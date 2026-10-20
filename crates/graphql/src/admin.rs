// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Content model management: `contentTypes`, `createContentType`, ...

use async_graphql::Value as GqlValue;
use async_graphql::dynamic::{Enum, Field, FieldFuture, FieldValue, InputObject, InputValue, Object, TypeRef};
use tess_core::model::{DEFAULT_PASSWORD_MIN_LENGTH, DEFAULT_TEXT_MAX_LENGTH, format_ts};
use tess_core::{CmsError, ContentType, ContentTypeDef, FieldDef, FieldType};
use serde_json::Value;

use crate::schema::{require_write, 
    TypeDef, admin_cms_err, admin_json_arg, admin_mark_dirty, admin_parse_def, admin_require,
};

const KINDS: &[&str] = &[
    "TEXT",
    "MARKDOWN",
    "INT",
    "FLOAT",
    "BOOLEAN",
    "DATETIME",
    "DATE",
    "JSON",
    "UUID",
    "PASSWORD",
    "ENUM",
    "RELATION",
    "COMPONENT",
    "DYNAMIC_ZONE",
];

pub(crate) fn kind_of(ft: &FieldType) -> &'static str {
    match ft {
        FieldType::Text { .. } => "TEXT",
        FieldType::Markdown => "MARKDOWN",
        FieldType::Int => "INT",
        FieldType::Float => "FLOAT",
        FieldType::Boolean => "BOOLEAN",
        FieldType::DateTime => "DATETIME",
        FieldType::Date => "DATE",
        FieldType::Json => "JSON",
        FieldType::Uuid => "UUID",
        FieldType::Password { .. } => "PASSWORD",
        FieldType::Enum { .. } => "ENUM",
        FieldType::Relation { .. } => "RELATION",
        FieldType::Component { .. } => "COMPONENT",
        FieldType::DynamicZone { .. } => "DYNAMIC_ZONE",
    }
}

/// Content types managed by the CMS itself; not editable through the API.
pub const BUILTIN_TYPES: &[&str] = &[tess_media::ASSET_TYPE];

fn check_not_builtin(api_id: &str) -> Result<(), CmsError> {
    if BUILTIN_TYPES.contains(&api_id) {
        return Err(CmsError::Conflict(format!("`{api_id}` is a built-in content type and cannot be modified")));
    }
    Ok(())
}

/// Optional length: absent → `default`, explicit null → none.
fn length(f: &serde_json::Map<String, Value>, key: &str, default: Option<u32>) -> Result<Option<u32>, CmsError> {
    match f.get(key) {
        None => Ok(default),
        Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| CmsError::validation(format!("`{key}` must be a positive integer"))),
    }
}

/// Parses `[ContentTypeFieldInput!]!` (content types and components).
pub(crate) fn parse_fields(input: Value) -> Result<Vec<FieldDef>, CmsError> {
    let bad = |m: &str| CmsError::validation(m.to_string());
    let str_opt = |o: &serde_json::Map<String, Value>, k: &str| o.get(k).and_then(Value::as_str).map(String::from);
    let strings = |o: &serde_json::Map<String, Value>, k: &str| -> Option<Vec<String>> {
        o.get(k)?.as_array().map(|xs| xs.iter().filter_map(|v| v.as_str().map(String::from)).collect())
    };
    let mut fields = Vec::new();
    for f in input.as_array().ok_or_else(|| bad("`fields` is required"))? {
        let f = f.as_object().ok_or_else(|| bad("field must be an object"))?;
        let name = str_opt(f, "name").ok_or_else(|| bad("field `name` is required"))?;
        let field_type = match f.get("type").and_then(Value::as_str).unwrap_or_default() {
            "TEXT" => FieldType::Text {
                min_length: length(f, "minLength", None)?.filter(|n| *n > 0),
                max_length: length(f, "maxLength", Some(DEFAULT_TEXT_MAX_LENGTH))?,
                pattern: str_opt(f, "pattern").filter(|p| !p.is_empty()),
            },
            "MARKDOWN" => FieldType::Markdown,
            "INT" => FieldType::Int,
            "FLOAT" => FieldType::Float,
            "BOOLEAN" => FieldType::Boolean,
            "DATETIME" => FieldType::DateTime,
            "DATE" => FieldType::Date,
            "JSON" => FieldType::Json,
            "UUID" => FieldType::Uuid,
            "PASSWORD" => FieldType::Password {
                min_length: length(f, "minLength", Some(DEFAULT_PASSWORD_MIN_LENGTH))?
                    .unwrap_or(DEFAULT_PASSWORD_MIN_LENGTH),
            },
            "ENUM" => FieldType::Enum {
                values: strings(f, "enumValues").ok_or_else(|| bad(&format!("ENUM field `{name}` needs `enumValues`")))?,
            },
            "RELATION" => FieldType::Relation {
                target: str_opt(f, "relationTarget")
                    .ok_or_else(|| bad(&format!("RELATION field `{name}` needs `relationTarget`")))?,
                many: f.get("many").and_then(Value::as_bool).unwrap_or(false),
            },
            "COMPONENT" => FieldType::Component {
                component: str_opt(f, "component")
                    .ok_or_else(|| bad(&format!("COMPONENT field `{name}` needs `component`")))?,
                repeatable: f.get("repeatable").and_then(Value::as_bool).unwrap_or(false),
            },
            "DYNAMIC_ZONE" => FieldType::DynamicZone {
                components: strings(f, "components")
                    .ok_or_else(|| bad(&format!("DYNAMIC_ZONE field `{name}` needs `components`")))?,
            },
            other => return Err(bad(&format!("unknown field type `{other}`"))),
        };
        fields.push(FieldDef {
            name,
            field_type,
            required: f.get("required").and_then(Value::as_bool).unwrap_or(false),
            description: str_opt(f, "description"),
        });
    }
    Ok(fields)
}

pub(crate) fn parse_content_type_input(input: Value) -> Result<ContentTypeDef, CmsError> {
    let bad = |m: &str| CmsError::validation(m.to_string());
    let obj = input.as_object().ok_or_else(|| bad("input must be an object"))?;
    let str_opt = |k: &str| obj.get(k).and_then(Value::as_str).map(String::from);
    Ok(ContentTypeDef {
        workflow: obj.get("workflow").and_then(Value::as_bool).unwrap_or(false),
        api_id: str_opt("apiId").ok_or_else(|| bad("`apiId` is required"))?,
        plural_api_id: str_opt("pluralApiId"),
        description: str_opt("description"),
        fields: parse_fields(obj.get("fields").cloned().unwrap_or(Value::Null))?,
    })
}

fn ct_field(name: &'static str, ty: TypeRef, get: fn(&ContentType) -> Option<GqlValue>) -> Field {
    Field::new(name, ty, move |ctx| {
        FieldFuture::new(async move {
            let ct = ctx.parent_value.try_downcast_ref::<ContentType>()?;
            Ok(get(ct).map(FieldValue::value))
        })
    })
}

fn def_field(name: &'static str, ty: TypeRef, get: fn(&FieldDef) -> Option<GqlValue>) -> Field {
    Field::new(name, ty, move |ctx| {
        FieldFuture::new(async move {
            let f = ctx.parent_value.try_downcast_ref::<FieldDef>()?;
            Ok(get(f).map(FieldValue::value))
        })
    })
}

fn types() -> Vec<TypeDef> {
    let content_type = Object::new("ContentType")
        .description("A content model. Each one generates its own GraphQL types, queries and mutations.")
        .field(ct_field("apiId", TypeRef::named_nn(TypeRef::STRING), |c| Some(c.api_id.clone().into())))
        .field(ct_field("pluralApiId", TypeRef::named_nn(TypeRef::STRING), |c| {
            Some(c.plural_api_id.clone().into())
        }))
        .field(ct_field("description", TypeRef::named(TypeRef::STRING), |c| c.description.clone().map(Into::into)))
        .field(
            ct_field("workflow", TypeRef::named_nn(TypeRef::BOOLEAN), |c| Some(c.workflow.into()))
                .description("Whether entries must be reviewed before publishing."),
        )
        .field(ct_field("createdAt", TypeRef::named_nn("DateTime"), |c| Some(format_ts(c.created_at).into())))
        .field(ct_field("updatedAt", TypeRef::named_nn("DateTime"), |c| Some(format_ts(c.updated_at).into())))
        .field(Field::new("fields", TypeRef::named_nn_list_nn("ContentTypeField"), |ctx| {
            FieldFuture::new(async move {
                let ct = ctx.parent_value.try_downcast_ref::<ContentType>()?;
                Ok(Some(FieldValue::list(ct.fields.iter().map(|f| FieldValue::borrowed_any(f)))))
            })
        }));

    let field = Object::new("ContentTypeField")
        .field(def_field("name", TypeRef::named_nn(TypeRef::STRING), |f| Some(f.name.clone().into())))
        .field(def_field("type", TypeRef::named_nn("FieldKind"), |f| {
            Some(GqlValue::Enum(async_graphql::Name::new(kind_of(&f.field_type))))
        }))
        .field(def_field("required", TypeRef::named_nn(TypeRef::BOOLEAN), |f| Some(f.required.into())))
        .field(def_field("description", TypeRef::named(TypeRef::STRING), |f| f.description.clone().map(Into::into)))
        .field(def_field("enumValues", TypeRef::named_nn_list(TypeRef::STRING), |f| match &f.field_type {
            FieldType::Enum { values } => Some(GqlValue::List(values.iter().cloned().map(Into::into).collect())),
            _ => None,
        }))
        .field(def_field("relationTarget", TypeRef::named(TypeRef::STRING), |f| match &f.field_type {
            FieldType::Relation { target, .. } => Some(target.clone().into()),
            _ => None,
        }))
        .field(def_field("minLength", TypeRef::named(TypeRef::INT), |f| match &f.field_type {
            FieldType::Text { min_length, .. } => min_length.map(|n| GqlValue::from(n as i64)),
            FieldType::Password { min_length } => Some((*min_length as i64).into()),
            _ => None,
        }))
        .field(
            def_field("maxLength", TypeRef::named(TypeRef::INT), |f| match &f.field_type {
                FieldType::Text { max_length, .. } => max_length.map(|n| GqlValue::from(n as i64)),
                _ => None,
            })
            .description("TEXT only; null means unlimited."),
        )
        .field(
            def_field("pattern", TypeRef::named(TypeRef::STRING), |f| match &f.field_type {
                FieldType::Text { pattern, .. } => pattern.clone().map(Into::into),
                _ => None,
            })
            .description("TEXT only: regular expression the whole value must match."),
        )
        .field(def_field("component", TypeRef::named(TypeRef::STRING), |f| match &f.field_type {
            FieldType::Component { component, .. } => Some(component.clone().into()),
            _ => None,
        }))
        .field(def_field("repeatable", TypeRef::named_nn(TypeRef::BOOLEAN), |f| {
            Some(matches!(f.field_type, FieldType::Component { repeatable: true, .. }).into())
        }))
        .field(def_field("components", TypeRef::named_nn_list(TypeRef::STRING), |f| match &f.field_type {
            FieldType::DynamicZone { components } => {
                Some(GqlValue::List(components.iter().cloned().map(Into::into).collect()))
            }
            _ => None,
        }))
        .field(def_field("many", TypeRef::named_nn(TypeRef::BOOLEAN), |f| {
            Some(matches!(f.field_type, FieldType::Relation { many: true, .. }).into())
        }));

    let field_input = InputObject::new("ContentTypeFieldInput")
        .field(InputValue::new("name", TypeRef::named_nn(TypeRef::STRING)))
        .field(InputValue::new("type", TypeRef::named_nn("FieldKind")))
        .field(InputValue::new("required", TypeRef::named(TypeRef::BOOLEAN)).default_value(false))
        .field(InputValue::new("description", TypeRef::named(TypeRef::STRING)))
        .field(InputValue::new("enumValues", TypeRef::named_nn_list(TypeRef::STRING)))
        .field(InputValue::new("relationTarget", TypeRef::named(TypeRef::STRING)))
        .field(InputValue::new("many", TypeRef::named(TypeRef::BOOLEAN)).default_value(false))
        .field(InputValue::new("minLength", TypeRef::named(TypeRef::INT)).description(
            "TEXT: minimum length (default 0). PASSWORD: minimum length (default 8).",
        ))
        .field(InputValue::new("maxLength", TypeRef::named(TypeRef::INT)).description(
            "TEXT: maximum length. Omitted: 80; null: unlimited.",
        ))
        .field(InputValue::new("pattern", TypeRef::named(TypeRef::STRING)).description(
            "TEXT: regular expression (Rust `regex` syntax) the whole value must match.",
        ))
        .field(InputValue::new("component", TypeRef::named(TypeRef::STRING)).description("COMPONENT: its apiId."))
        .field(InputValue::new("repeatable", TypeRef::named(TypeRef::BOOLEAN)).default_value(false))
        .field(
            InputValue::new("components", TypeRef::named_nn_list(TypeRef::STRING))
                .description("DYNAMIC_ZONE: the allowed components."),
        );

    let input = InputObject::new("ContentTypeInput")
        .field(InputValue::new("apiId", TypeRef::named_nn(TypeRef::STRING)))
        .field(InputValue::new("pluralApiId", TypeRef::named(TypeRef::STRING)))
        .field(InputValue::new("description", TypeRef::named(TypeRef::STRING)))
        .field(InputValue::new("workflow", TypeRef::named(TypeRef::BOOLEAN)).default_value(false))
        .field(InputValue::new("fields", TypeRef::named_nn_list_nn("ContentTypeFieldInput")));

    vec![
        TypeDef::Object(content_type),
        TypeDef::Object(field),
        TypeDef::Input(field_input),
        TypeDef::Input(input),
        TypeDef::Enum(Enum::new("FieldKind").items(KINDS.iter().copied())),
    ]
}

pub(crate) fn definitions() -> (Vec<Field>, Vec<Field>, Vec<TypeDef>) {
    let list = Field::new("contentTypes", TypeRef::named_nn_list_nn("ContentType"), |ctx| {
        FieldFuture::new(async move {
            // Readable by every signed-in user (forms in the content manager).
            require_write(&ctx)?;
            let all = ctx.data::<tess_core::Cms>()?.content_types().await.map_err(admin_cms_err)?;
            Ok(Some(FieldValue::list(all.into_iter().map(FieldValue::owned_any))))
        })
    });

    let get = Field::new("contentType", TypeRef::named("ContentType"), |ctx| {
        FieldFuture::new(async move {
            // Readable by every signed-in user (forms in the content manager).
            require_write(&ctx)?;
            let api_id = ctx.args.try_get("apiId")?.string()?;
            match ctx.data::<tess_core::Cms>()?.content_type(api_id).await {
                Ok(ct) => Ok(Some(FieldValue::owned_any(ct))),
                Err(CmsError::NotFound(_)) => Ok(None),
                Err(e) => Err(admin_cms_err(e)),
            }
        })
    })
    .argument(InputValue::new("apiId", TypeRef::named_nn(TypeRef::STRING)));

    let create = Field::new("createContentType", TypeRef::named_nn("ContentType"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let def = admin_parse_def(admin_json_arg(&ctx, "input")?).map_err(admin_cms_err)?;
            check_not_builtin(&def.api_id).map_err(admin_cms_err)?;
            let ct = ctx.data::<tess_core::Cms>()?.create_content_type(def, &actor).await.map_err(admin_cms_err)?;
            admin_mark_dirty(&ctx)?;
            Ok(Some(FieldValue::owned_any(ct)))
        })
    })
    .argument(InputValue::new("input", TypeRef::named_nn("ContentTypeInput")));

    let update = Field::new("updateContentType", TypeRef::named_nn("ContentType"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let api_id = ctx.args.try_get("apiId")?.string()?.to_string();
            check_not_builtin(&api_id).map_err(admin_cms_err)?;
            let def = admin_parse_def(admin_json_arg(&ctx, "input")?).map_err(admin_cms_err)?;
            let ct =
                ctx.data::<tess_core::Cms>()?.update_content_type(&api_id, def, &actor).await.map_err(admin_cms_err)?;
            admin_mark_dirty(&ctx)?;
            Ok(Some(FieldValue::owned_any(ct)))
        })
    })
    .description("Replace a content type's definition (renaming is not supported).")
    .argument(InputValue::new("apiId", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("input", TypeRef::named_nn("ContentTypeInput")));

    let delete = Field::new("deleteContentType", TypeRef::named_nn(TypeRef::STRING), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let api_id = ctx.args.try_get("apiId")?.string()?.to_string();
            check_not_builtin(&api_id).map_err(admin_cms_err)?;
            ctx.data::<tess_core::Cms>()?.delete_content_type(&api_id, &actor).await.map_err(admin_cms_err)?;
            admin_mark_dirty(&ctx)?;
            Ok(Some(FieldValue::value(api_id)))
        })
    })
    .description("Delete a content type and ALL of its entries.")
    .argument(InputValue::new("apiId", TypeRef::named_nn(TypeRef::STRING)));

    (vec![list, get], vec![create, update, delete], types())
}
