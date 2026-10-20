// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! GraphQL types, resolvers and input handling of content fields, shared by
//! content types and components.

use async_graphql::dynamic::{Field, FieldFuture, FieldValue, ResolverContext, TypeRef};
use async_graphql::{Name, Value as GqlValue};
use serde_json::{Map, Value};
use tess_core::model::{COMPONENT_KEY, upper_first};
use tess_core::query::{EntryQuery, Filter, Op};
use tess_core::validate::Components;
use tess_core::{Cms, CmsError, FieldDef, FieldType, Stage};
use uuid::Uuid;

use crate::ids::IdCodec;
use crate::schema::{EntryNode, GqlResult, cms_err, id_err};

/// Parent value of component objects: their data and the stage of the entry
/// they belong to (relations inside resolve in that stage).
pub(crate) struct ComponentNode {
    pub(crate) data: Map<String, Value>,
    pub(crate) stage: Stage,
}

/// Data and stage of the parent object, whether an entry or a component.
fn parent<'a>(ctx: &ResolverContext<'a>) -> GqlResult<(&'a Map<String, Value>, Stage)> {
    if let Ok(node) = ctx.parent_value.try_downcast_ref::<EntryNode>() {
        return Ok((&node.view, node.stage));
    }
    let node = ctx.parent_value.try_downcast_ref::<ComponentNode>()?;
    Ok((&node.data, node.stage))
}

pub(crate) fn enum_type_name(owner: &str, field: &str) -> String {
    format!("{owner}{}Enum", upper_first(field))
}

pub(crate) fn zone_type_name(owner: &str, field: &str) -> String {
    format!("{owner}{}Zone", upper_first(field))
}

pub(crate) fn zone_input_name(owner: &str, field: &str) -> String {
    format!("{owner}{}ZoneInput", upper_first(field))
}

pub(crate) fn component_input_name(component: &str) -> String {
    format!("{component}Input")
}

/// GraphQL type of a field's scalar value (filters, inputs). Relations use `ID`.
pub(crate) fn scalar_type_name(owner: &str, f: &FieldDef) -> String {
    match &f.field_type {
        FieldType::Text { .. } | FieldType::Markdown | FieldType::Password { .. } => {
            TypeRef::STRING.into()
        }
        FieldType::Int => TypeRef::INT.into(),
        FieldType::Float => TypeRef::FLOAT.into(),
        FieldType::Boolean => TypeRef::BOOLEAN.into(),
        FieldType::DateTime => "DateTime".into(),
        FieldType::Date => "Date".into(),
        FieldType::Uuid => "UUID".into(),
        FieldType::Json => "JSON".into(),
        FieldType::Enum { .. } => enum_type_name(owner, &f.name),
        FieldType::Relation { .. } => TypeRef::ID.into(),
        FieldType::Component { component, .. } => component.clone(),
        FieldType::DynamicZone { .. } => zone_type_name(owner, &f.name),
    }
}

/// Input type of a field in create (`required` honored) and update inputs.
pub(crate) fn input_type(owner: &str, f: &FieldDef, required: bool) -> TypeRef {
    let list = |name: String| {
        if required {
            TypeRef::named_nn_list_nn(name)
        } else {
            TypeRef::named_nn_list(name)
        }
    };
    let one = |name: String| {
        if required {
            TypeRef::named_nn(name)
        } else {
            TypeRef::named(name)
        }
    };
    match &f.field_type {
        FieldType::Relation { many: true, .. } => list(TypeRef::ID.into()),
        FieldType::Component {
            component,
            repeatable: true,
        } => list(component_input_name(component)),
        FieldType::Component {
            component,
            repeatable: false,
        } => one(component_input_name(component)),
        FieldType::DynamicZone { .. } => list(zone_input_name(owner, &f.name)),
        _ => one(scalar_type_name(owner, f)),
    }
}

/// Converts a stored scalar to its GraphQL output, or `None` when it does
/// not match the field's current type (e.g. after the model changed).
fn output_scalar(field_type: &FieldType, v: &Value) -> Option<GqlValue> {
    match (field_type, v) {
        (
            FieldType::Text { .. }
            | FieldType::Markdown
            | FieldType::DateTime
            | FieldType::Date
            | FieldType::Uuid,
            Value::String(s),
        ) => Some(GqlValue::String(s.clone())),
        (FieldType::Int, Value::Number(n)) => n.as_i64().map(|i| GqlValue::Number(i.into())),
        (FieldType::Float, Value::Number(n)) => n
            .as_f64()
            .and_then(async_graphql::Number::from_f64)
            .map(GqlValue::Number),
        (FieldType::Boolean, Value::Bool(b)) => Some(GqlValue::Boolean(*b)),
        (FieldType::Enum { values }, Value::String(s)) if values.contains(s) => {
            Some(GqlValue::Enum(Name::new(s)))
        }
        (FieldType::Json, v) => GqlValue::from_json(v.clone()).ok(),
        _ => None,
    }
}

/// Output field for a content or component field; `None` for write-only
/// fields (passwords).
pub(crate) fn output_field(owner: &str, f: &FieldDef) -> Option<Field> {
    if f.field_type.is_secret() {
        return None;
    }
    let ty = match &f.field_type {
        FieldType::Relation {
            target,
            many: false,
        } => TypeRef::named(target),
        FieldType::Relation { target, many: true } => TypeRef::named_nn_list_nn(target),
        FieldType::Component {
            component,
            repeatable: false,
        } => TypeRef::named(component),
        FieldType::Component {
            component,
            repeatable: true,
        } => TypeRef::named_nn_list_nn(component),
        FieldType::DynamicZone { .. } => TypeRef::named_nn_list_nn(zone_type_name(owner, &f.name)),
        _ => TypeRef::named(scalar_type_name(owner, f)),
    };
    let def = f.clone();
    let mut field = Field::new(&f.name, ty, move |ctx| {
        let def = def.clone();
        FieldFuture::new(async move {
            let (data, stage) = parent(&ctx)?;
            let raw = data.get(&def.name);
            match &def.field_type {
                FieldType::Relation { target, many } => {
                    resolve_relation(ctx.data::<Cms>()?, target, *many, raw, stage).await
                }
                FieldType::Component {
                    repeatable: false, ..
                } => Ok(raw.and_then(Value::as_object).map(|m| {
                    FieldValue::owned_any(ComponentNode {
                        data: m.clone(),
                        stage,
                    })
                })),
                FieldType::Component {
                    repeatable: true, ..
                } => Ok(Some(FieldValue::list(
                    raw.and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_object)
                        .map(|m| {
                            FieldValue::owned_any(ComponentNode {
                                data: m.clone(),
                                stage,
                            })
                        })
                        .collect::<Vec<_>>(),
                ))),
                FieldType::DynamicZone { components } => Ok(Some(FieldValue::list(
                    raw.and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_object)
                        .filter_map(|m| {
                            // Items of components no longer allowed are skipped.
                            let name = m.get(COMPONENT_KEY)?.as_str()?.to_string();
                            components.contains(&name).then(|| {
                                FieldValue::owned_any(ComponentNode {
                                    data: m.clone(),
                                    stage,
                                })
                                .with_type(name)
                            })
                        })
                        .collect::<Vec<_>>(),
                ))),
                ft => Ok(raw
                    .and_then(|v| output_scalar(ft, v))
                    .map(FieldValue::value)),
            }
        })
    });
    if let Some(d) = &f.description {
        field = field.description(d);
    }
    Some(field)
}

/// Loads related entries in the same stage as the parent. Missing targets
/// (deleted or not published) are skipped.
pub(crate) async fn resolve_relation(
    cms: &Cms,
    target: &str,
    many: bool,
    raw: Option<&Value>,
    stage: Stage,
) -> GqlResult<Option<FieldValue<'static>>> {
    let ids: Vec<String> = match raw {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(xs)) => xs
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        _ => vec![],
    };
    if !many {
        let Some(id) = ids.first().and_then(|s| Uuid::parse_str(s).ok()) else {
            return Ok(None);
        };
        let entry = cms.get_entry(target, id, stage).await.map_err(cms_err)?;
        return Ok(entry.and_then(|e| EntryNode::value(e, stage)));
    }
    if ids.is_empty() {
        return Ok(Some(FieldValue::list(Vec::<FieldValue>::new())));
    }
    let mut q = EntryQuery::new(stage);
    q.limit = ids.len();
    q.filter = Some(Filter::cmp(
        "id",
        Op::In,
        Value::Array(ids.iter().cloned().map(Value::String).collect()),
    ));
    let mut found = cms.query_entries(target, &q).await.map_err(cms_err)?.items;
    let ordered = ids.iter().filter_map(|id| {
        let pos = found.iter().position(|e| &e.id.to_string() == id)?;
        EntryNode::value(found.swap_remove(pos), stage)
    });
    Ok(Some(FieldValue::list(ordered.collect::<Vec<_>>())))
}

/// Prepares client input for the CMS, recursively: opaque relation ids
/// become storage UUIDs, and dynamic-zone items `{ Hero: {...} }` become
/// `{ "__component": "Hero", ... }`.
pub(crate) fn decode_input(
    fields: &[FieldDef],
    data: &mut Map<String, Value>,
    codec: &IdCodec,
    components: &Components,
) -> GqlResult<()> {
    for f in fields {
        let Some(value) = data.get_mut(&f.name) else {
            continue;
        };
        match &f.field_type {
            FieldType::Relation { target, .. } => {
                let decode = |v: &mut Value| -> GqlResult<()> {
                    if let Value::String(token) = v {
                        let (_, id) = codec.decode(token, Some(target)).map_err(id_err)?;
                        *token = id.to_string();
                    }
                    Ok(())
                };
                match value {
                    Value::Array(items) => items.iter_mut().try_for_each(decode)?,
                    v => decode(v)?,
                }
            }
            FieldType::Component { component, .. } => {
                let Some(def) = components.get(component) else {
                    continue;
                };
                let items: Vec<&mut Value> = match value {
                    Value::Array(items) => items.iter_mut().collect(),
                    v => vec![v],
                };
                for item in items {
                    if let Value::Object(m) = item {
                        decode_input(&def.fields, m, codec, components)?;
                    }
                }
            }
            FieldType::DynamicZone { .. } => {
                let Value::Array(items) = value else { continue };
                for item in items.iter_mut() {
                    let Value::Object(choice) = item else {
                        continue;
                    };
                    if choice.len() != 1 {
                        return Err(cms_err(CmsError::validation(format!(
                            "each `{}` item must set exactly one component",
                            f.name
                        ))));
                    }
                    let (name, inner) = choice.iter_mut().next().expect("one entry");
                    let name = name.clone();
                    let mut inner = match inner.take() {
                        Value::Object(m) => m,
                        _ => Map::new(),
                    };
                    if let Some(def) = components.get(&name) {
                        decode_input(&def.fields, &mut inner, codec, components)?;
                    }
                    inner.insert(COMPONENT_KEY.into(), Value::String(name));
                    *item = Value::Object(inner);
                }
            }
            _ => {}
        }
    }
    Ok(())
}
