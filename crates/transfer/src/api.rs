// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Minimal GraphQL client for the operations the transfer tool needs.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::document::{ComponentDoc, FieldDef, ModelDef, UserDef};

const FIELD_FIELDS: &str = "fields { name type required description enumValues relationTarget many minLength maxLength pattern component repeatable components }";

pub struct Client {
    http: reqwest::Client,
    endpoint: String,
    token: String,
}

/// A user on the server: its opaque id and its exportable definition.
pub struct RemoteUser {
    pub id: String,
    pub def: UserDef,
}

impl Client {
    pub fn new(url: &str, token: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        Ok(Client {
            http,
            endpoint: format!("{}/graphql", url.trim_end_matches('/')),
            token,
        })
    }

    async fn gql(&self, query: &str, variables: Value) -> Result<Value> {
        let res = self
            .http
            .post(&self.endpoint)
            .bearer_auth(&self.token)
            .json(&json!({ "query": query, "variables": variables }))
            .send()
            .await
            .with_context(|| format!("cannot reach {}", self.endpoint))?;
        match res.status().as_u16() {
            401 => bail!("invalid, expired or revoked token (HTTP 401)"),
            s if !(200..300).contains(&s) => bail!("HTTP {s} from {}", self.endpoint),
            _ => {}
        }
        let mut body: Value = res.json().await.context("invalid response")?;
        if let Some(err) = body.get("errors").and_then(|e| e.get(0)) {
            let code = err
                .pointer("/extensions/code")
                .and_then(Value::as_str)
                .unwrap_or("ERROR");
            let message = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            bail!("{code}: {message}");
        }
        Ok(body["data"].take())
    }

    /// The API returns nulls for unset options: drop them, except the
    /// `maxLength` of TEXT fields, where null means "unlimited".
    fn normalize_remote(mut items: Vec<Value>) -> Vec<Value> {
        for item in &mut items {
            for f in item["fields"].as_array_mut().into_iter().flatten() {
                let is_text = f["type"] == "TEXT";
                if let Some(o) = f.as_object_mut() {
                    o.retain(|k, v| !v.is_null() || (is_text && k == "maxLength"));
                    if !is_text {
                        o.remove("maxLength");
                    }
                }
            }
            if let Some(o) = item.as_object_mut() {
                o.retain(|k, v| !v.is_null() && k != "createdAt" && k != "updatedAt");
            }
        }
        items
    }

    pub async fn models(&self) -> Result<Vec<ModelDef>> {
        let q = format!(
            "{{ contentTypes {{ apiId pluralApiId description workflow {FIELD_FIELDS} }} }}"
        );
        let mut data = self.gql(&q, json!({})).await?;
        let items: Vec<Value> = serde_json::from_value(data["contentTypes"].take())?;
        Self::normalize_remote(items)
            .into_iter()
            .map(|m| Ok(serde_json::from_value(m)?))
            .collect()
    }

    pub async fn components(&self) -> Result<Vec<ComponentDoc>> {
        let q = format!("{{ components {{ apiId description {FIELD_FIELDS} }} }}");
        let mut data = self.gql(&q, json!({})).await?;
        let items: Vec<Value> = serde_json::from_value(data["components"].take())?;
        Self::normalize_remote(items)
            .into_iter()
            .map(|c| Ok(serde_json::from_value(c)?))
            .collect()
    }

    pub async fn create_component(&self, c: &ComponentDoc) -> Result<()> {
        self.gql(
            "mutation($input: ComponentDefinitionInput!) { createComponent(input: $input) { apiId } }",
            json!({ "input": Self::component_input(c) }),
        )
        .await?;
        Ok(())
    }

    pub async fn update_component(&self, c: &ComponentDoc) -> Result<()> {
        self.gql(
            "mutation($apiId: String!, $input: ComponentDefinitionInput!) { updateComponent(apiId: $apiId, input: $input) { apiId } }",
            json!({ "apiId": c.api_id, "input": Self::component_input(c) }),
        )
        .await?;
        Ok(())
    }

    fn component_input(c: &ComponentDoc) -> Value {
        json!({ "apiId": c.api_id, "description": c.description, "fields": Self::fields_input(&c.fields) })
    }

    fn fields_input(fields: &[FieldDef]) -> Vec<Value> {
        fields
            .iter()
            .map(|f| {
                let mut v = json!({
                    "name": f.name,
                    "type": f.kind,
                    "required": f.required,
                    "description": f.description,
                    "enumValues": f.enum_values,
                    "relationTarget": f.relation_target,
                    "many": f.many,
                    "minLength": f.min_length,
                    "pattern": f.pattern,
                    "component": f.component,
                    "repeatable": f.repeatable,
                    "components": f.components,
                });
                // Absent: server default (80); null: unlimited.
                if let Some(max) = f.max_length {
                    v["maxLength"] = json!(max);
                }
                v
            })
            .collect()
    }

    pub async fn users(&self) -> Result<Vec<RemoteUser>> {
        let mut data = self
            .gql("{ users { id email name role scope disabled } }", json!({}))
            .await?;
        let users: Vec<Value> = serde_json::from_value(data["users"].take())?;
        users
            .into_iter()
            .map(|mut u| {
                let id = u["id"].as_str().unwrap_or_default().to_string();
                u.as_object_mut().map(|o| o.remove("id"));
                Ok(RemoteUser {
                    id,
                    def: serde_json::from_value(u)?,
                })
            })
            .collect()
    }

    fn model_input(m: &ModelDef) -> Value {
        json!({
            "apiId": m.api_id,
            "pluralApiId": m.plural_api_id,
            "description": m.description,
            "workflow": m.workflow,
            "fields": Self::fields_input(&m.fields),
        })
    }

    pub async fn create_model(&self, m: &ModelDef) -> Result<()> {
        self.gql(
            "mutation($input: ContentTypeInput!) { createContentType(input: $input) { apiId } }",
            json!({ "input": Self::model_input(m) }),
        )
        .await?;
        Ok(())
    }

    /// Replaces the whole definition (that is how `updateContentType` works).
    pub async fn update_model(&self, m: &ModelDef) -> Result<()> {
        self.gql(
            "mutation($apiId: String!, $input: ContentTypeInput!) { updateContentType(apiId: $apiId, input: $input) { apiId } }",
            json!({ "apiId": m.api_id, "input": Self::model_input(m) }),
        )
        .await?;
        Ok(())
    }

    fn display_name(u: &UserDef) -> String {
        u.name
            .clone()
            .unwrap_or_else(|| u.email.split('@').next().unwrap_or("user").to_string())
    }

    pub async fn create_user(&self, u: &UserDef, password: &str) -> Result<()> {
        let mut data = self
            .gql(
                "mutation($i: UserInput!) { createUser(input: $i) { id } }",
                json!({ "i": {
                    "email": u.email, "name": Self::display_name(u), "role": u.role,
                    "scope": u.scope, "password": password,
                }}),
            )
            .await?;
        if u.disabled {
            let id = data["createUser"]["id"].take();
            self.gql(
                "mutation($id: ID!) { updateUser(id: $id, input: {disabled: true}) { id } }",
                json!({ "id": id }),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn update_user(&self, id: &str, u: &UserDef) -> Result<()> {
        self.gql(
            "mutation($id: ID!, $i: UserUpdateInput!) { updateUser(id: $id, input: $i) { id } }",
            json!({ "id": id, "i": {
                "name": Self::display_name(u), "role": u.role, "scope": u.scope, "disabled": u.disabled,
            }}),
        )
        .await?;
        Ok(())
    }
}
