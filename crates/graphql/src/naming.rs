// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Names of everything generated per content type, and the guard rejecting
//! models whose generated names would collide.

use std::collections::HashSet;

use tess_core::model::upper_first;
use tess_core::validate::Components;
use tess_core::{CmsError, ContentType, FieldType, Result};

use crate::fields::{component_input_name, enum_type_name, zone_input_name, zone_type_name};

/// Types defined by the CMS itself (plus GraphQL built-ins).
pub const FIXED_TYPES: &[&str] = &[
    "Query",
    "Mutation",
    "Subscription",
    "Stage",
    "DateTime",
    "JSON",
    "ContentType",
    "ContentTypeField",
    "ContentTypeInput",
    "ContentTypeFieldInput",
    "FieldKind",
    "HttpHeader",
    "AssetUpload",
    "Plugin",
    "PluginLimits",
    "PluginSettingsInput",
    "Date",
    "UUID",
    "ComponentDefinition",
    "ComponentDefinitionInput",
    "WorkflowStatus",
    "HistoryAction",
    "HistoryEvent",
    "Role",
    "User",
    "Session",
    "ApiToken",
    "CreatedApiToken",
    "UserInput",
    "UserUpdateInput",
    "String",
    "Int",
    "Float",
    "Boolean",
    "ID",
];
pub const FIXED_QUERIES: &[&str] = &[
    "contentTypes",
    "contentType",
    "plugins",
    "plugin",
    "pluginLimits",
    "me",
    "users",
    "apiTokens",
    "setupRequired",
    "components",
    "component",
    "verifyPassword",
];
pub const FIXED_MUTATIONS: &[&str] = &[
    "createContentType",
    "updateContentType",
    "deleteContentType",
    "createAssetUpload",
    "confirmAssetUpload",
    "installPlugin",
    "updatePlugin",
    "uninstallPlugin",
    "reindexSearch",
    "login",
    "changePassword",
    "signOutEverywhere",
    "createUser",
    "updateUser",
    "deleteUser",
    "createApiToken",
    "revokeApiToken",
    "createComponent",
    "updateComponent",
    "deleteComponent",
];

pub struct Names {
    pub object: String,
    pub connection: String,
    pub where_input: String,
    pub order_by: String,
    pub create_input: String,
    pub update_input: String,
    pub single_query: String,
    pub list_query: String,
    pub search_query: String,
    pub create: String,
    pub update: String,
    pub delete: String,
    pub publish: String,
    pub unpublish: String,
    pub submit: String,
    pub approve: String,
    pub request_changes: String,
}

impl Names {
    pub fn of(ct: &ContentType) -> Self {
        let t = &ct.api_id;
        Names {
            object: t.clone(),
            connection: format!("{t}Connection"),
            where_input: format!("{t}WhereInput"),
            order_by: format!("{t}OrderByInput"),
            create_input: format!("{t}CreateInput"),
            update_input: format!("{t}UpdateInput"),
            single_query: ct.singular_api_id(),
            list_query: ct.plural_api_id.clone(),
            search_query: format!("search{}", upper_first(&ct.plural_api_id)),
            create: format!("create{t}"),
            update: format!("update{t}"),
            delete: format!("delete{t}"),
            publish: format!("publish{t}"),
            unpublish: format!("unpublish{t}"),
            submit: format!("submit{t}ForReview"),
            approve: format!("approve{t}"),
            request_changes: format!("requestChanges{t}"),
        }
    }
}

/// [`tess_core::service::SchemaGuard`] ensuring the GraphQL schema stays buildable.
pub fn check_conflicts(content_types: &[ContentType], components: &Components) -> Result<()> {
    let mut types: HashSet<String> = FIXED_TYPES.iter().map(|s| s.to_string()).collect();
    let mut queries: HashSet<String> = FIXED_QUERIES.iter().map(|s| s.to_string()).collect();
    let mut mutations: HashSet<String> = FIXED_MUTATIONS.iter().map(|s| s.to_string()).collect();
    let clash = |what: &str, owner: &str, kind: &str, name: &str| {
        Err(CmsError::Conflict(format!("{what} `{owner}` generates {kind} `{name}`, which is already used")))
    };
    for c in components.values() {
        let mut generated = vec![c.api_id.clone(), component_input_name(&c.api_id)];
        for f in &c.fields {
            if matches!(f.field_type, FieldType::Enum { .. }) {
                generated.push(enum_type_name(&c.api_id, &f.name));
            }
        }
        for name in generated {
            if !types.insert(name.clone()) {
                return clash("component", &c.api_id, "type", &name);
            }
        }
    }
    for ct in content_types {
        let n = Names::of(ct);
        let mut generated = vec![n.object, n.connection, n.where_input, n.order_by, n.create_input, n.update_input];
        for f in &ct.fields {
            match f.field_type {
                FieldType::Enum { .. } => generated.push(enum_type_name(&ct.api_id, &f.name)),
                FieldType::DynamicZone { .. } => {
                    generated.push(zone_type_name(&ct.api_id, &f.name));
                    generated.push(zone_input_name(&ct.api_id, &f.name));
                }
                _ => {}
            }
        }
        for name in generated {
            if !types.insert(name.clone()) {
                return clash("content type", &ct.api_id, "type", &name);
            }
        }
        for name in [n.single_query, n.list_query, n.search_query] {
            if !queries.insert(name.clone()) {
                return clash("content type", &ct.api_id, "query", &name);
            }
        }
        // Workflow mutations are reserved even when the workflow is off, so
        // enabling it later can never clash.
        for name in [n.create, n.update, n.delete, n.publish, n.unpublish, n.submit, n.approve, n.request_changes] {
            if !mutations.insert(name.clone()) {
                return clash("content type", &ct.api_id, "mutation", &name);
            }
        }
    }
    Ok(())
}
