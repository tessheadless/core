// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! The export file format (YAML or JSON). Defaults (`false`, empty lists,
//! missing descriptions) are omitted to keep files short and diff-friendly.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;

fn is_false(b: &bool) -> bool {
    !*b
}

fn default_version() -> u32 {
    FORMAT_VERSION
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Document {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<ComponentDoc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_types: Option<Vec<ModelDef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub users: Option<Vec<UserDef>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelDef {
    pub api_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural_api_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub workflow: bool,
    pub fields: Vec<FieldDef>,
}

/// A reusable group of fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentDoc {
    pub api_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub fields: Vec<FieldDef>,
}

/// `Some(None)` for an explicit `null` (e.g. unlimited `maxLength`), `None`
/// when the key is absent (server default).
fn double_option<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<u32>>, D::Error> {
    Option::<u32>::deserialize(d).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldDef {
    pub name: String,
    /// TEXT, MARKDOWN, INT, FLOAT, BOOLEAN, DATETIME, DATE, JSON, UUID,
    /// PASSWORD, ENUM, RELATION, COMPONENT or DYNAMIC_ZONE.
    #[serde(rename = "type")]
    pub kind: String,
    /// TEXT and PASSWORD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    /// TEXT: absent = server default (80), `null` = unlimited.
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_length: Option<Option<u32>>,
    /// TEXT: regex the whole value must match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// COMPONENT: its apiId.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub repeatable: bool,
    /// DYNAMIC_ZONE: allowed components.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_target: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub many: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserDef {
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// OWNER, ADMIN, CONTRIBUTOR, REVIEWER or MEMBER.
    pub role: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
    /// Optional initial password for new users. Never exported.
    #[serde(default, skip_serializing)]
    pub password: Option<String>,
}

pub fn relation_targets(fields: &[FieldDef]) -> impl Iterator<Item = &str> {
    fields
        .iter()
        .filter(|f| f.kind == "RELATION")
        .filter_map(|f| f.relation_target.as_deref())
}

/// Components used by COMPONENT and DYNAMIC_ZONE fields.
pub fn component_refs(fields: &[FieldDef]) -> impl Iterator<Item = &str> {
    fields.iter().flat_map(|f| {
        f.component
            .as_deref()
            .into_iter()
            .chain(f.components.iter().flatten().map(String::as_str))
    })
}

/// The fields without the relations pointing to `targets`.
pub fn without_relations_to(fields: &[FieldDef], targets: &BTreeSet<String>) -> Vec<FieldDef> {
    fields
        .iter()
        .filter(|f| {
            !(f.kind == "RELATION"
                && f.relation_target
                    .as_ref()
                    .is_some_and(|t| targets.contains(t)))
        })
        .cloned()
        .collect()
}

impl ModelDef {
    pub fn relation_targets(&self) -> impl Iterator<Item = &str> {
        relation_targets(&self.fields)
    }

    /// The same model without its relation fields pointing to `targets`.
    pub fn without_relations_to(&self, targets: &BTreeSet<String>) -> ModelDef {
        ModelDef {
            fields: without_relations_to(&self.fields, targets),
            ..self.clone()
        }
    }
}

impl ComponentDoc {
    pub fn without_relations_to(&self, targets: &BTreeSet<String>) -> ComponentDoc {
        ComponentDoc {
            fields: without_relations_to(&self.fields, targets),
            ..self.clone()
        }
    }
}

impl Document {
    pub fn render(&self, json: bool) -> Result<String> {
        Ok(if json {
            serde_json::to_string_pretty(self)? + "\n"
        } else {
            serde_norway::to_string(self)?
        })
    }

    pub fn parse(text: &str, json: bool) -> Result<Document> {
        let doc: Document = if json {
            serde_json::from_str(text).context("invalid JSON")?
        } else {
            serde_norway::from_str(text).context("invalid YAML")?
        };
        if doc.version != FORMAT_VERSION {
            bail!(
                "unsupported file version {} (expected {FORMAT_VERSION})",
                doc.version
            );
        }
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const YAML: &str = "\
version: 1
contentTypes:
- apiId: Post
  pluralApiId: posts
  workflow: true
  fields:
  - name: title
    type: TEXT
    required: true
    maxLength: null
  - name: tags
    type: RELATION
    relationTarget: Tag
    many: true
users:
- email: ada@example.com
  name: Ada Example
  role: OWNER
  password: only-for-import
";

    #[test]
    fn yaml_and_json_round_trip() {
        let doc = Document::parse(YAML, false).unwrap();
        let post = &doc.content_types.as_ref().unwrap()[0];
        assert!(post.workflow);
        assert_eq!(post.relation_targets().collect::<Vec<_>>(), vec!["Tag"]);
        assert_eq!(
            doc.users.as_ref().unwrap()[0].password.as_deref(),
            Some("only-for-import")
        );

        // Passwords are never written back.
        for json in [false, true] {
            let text = doc.render(json).unwrap();
            assert!(!text.contains("only-for-import"));
            let again = Document::parse(&text, json).unwrap();
            assert_eq!(again.content_types, doc.content_types);
        }
        // `maxLength: null` (unlimited) survives; absent stays absent.
        let text = doc.render(false).unwrap();
        assert!(text.contains("maxLength: null"), "{text}");
        assert_eq!(
            Document::parse(&text, false)
                .unwrap()
                .content_types
                .unwrap()[0]
                .fields[0]
                .max_length,
            Some(None)
        );
        assert_eq!(
            doc.content_types.as_ref().unwrap()[0].fields[1].max_length,
            None
        );
        // Defaults are omitted.
        let text = doc.render(false).unwrap();
        assert!(!text.contains("many: false") && !text.contains("required: false"));
    }

    #[test]
    fn rejects_unknown_keys_and_versions() {
        assert!(Document::parse("version: 2\n", false).is_err());
        assert!(
            Document::parse(
                "contentTypes: [{apiId: A, fields: [], colour: red}]\n",
                false
            )
            .is_err()
        );
    }

    #[test]
    fn strips_deferred_relations() {
        let doc = Document::parse(YAML, false).unwrap();
        let post = &doc.content_types.unwrap()[0];
        let stripped = post.without_relations_to(&BTreeSet::from(["Tag".to_string()]));
        assert_eq!(stripped.fields.len(), 1);
    }
}
