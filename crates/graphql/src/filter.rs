// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Translation of `XxxWhereInput` / `XxxOrderByInput` arguments into the
//! backend-neutral query language.

use tess_core::model::{format_ts, parse_ts};
use tess_core::query::{Direction, Filter, Op, SortKey};
use tess_core::{CmsError, ContentType, FieldType, Result};
use serde_json::Value;

/// Operator suffixes available in where inputs (`title_contains`, ...).
pub const SUFFIXES: &[(&str, Op)] = &[
    ("not", Op::Ne),
    ("in", Op::In),
    ("not_in", Op::NotIn),
    ("lt", Op::Lt),
    ("lte", Op::Lte),
    ("gt", Op::Gt),
    ("gte", Op::Gte),
    ("contains", Op::Contains),
    ("not_contains", Op::Contains),
    ("starts_with", Op::StartsWith),
    ("ends_with", Op::EndsWith),
];

/// Kind of a filterable field, which decides the operators it supports.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    Id,
    Text,
    Number,
    DateTime,
    Date,
    Uuid,
    Boolean,
    Enum,
}

impl FilterKind {
    pub fn of(field_type: &FieldType) -> Option<Self> {
        Some(match field_type {
            FieldType::Text { .. } | FieldType::Markdown => FilterKind::Text,
            FieldType::Int | FieldType::Float => FilterKind::Number,
            FieldType::DateTime => FilterKind::DateTime,
            FieldType::Date => FilterKind::Date,
            FieldType::Uuid => FilterKind::Uuid,
            FieldType::Boolean => FilterKind::Boolean,
            FieldType::Enum { .. } => FilterKind::Enum,
            FieldType::Relation { many: false, .. } => FilterKind::Id,
            FieldType::Relation { many: true, .. }
            | FieldType::Json
            | FieldType::Password { .. }
            | FieldType::Component { .. }
            | FieldType::DynamicZone { .. } => return None,
        })
    }

    pub fn suffixes(self) -> &'static [&'static str] {
        match self {
            FilterKind::Id | FilterKind::Uuid | FilterKind::Enum => &["not", "in", "not_in"],
            FilterKind::Boolean => &["not"],
            FilterKind::Number | FilterKind::DateTime | FilterKind::Date => {
                &["not", "in", "not_in", "lt", "lte", "gt", "gte"]
            }
            FilterKind::Text => &[
                "not",
                "in",
                "not_in",
                "contains",
                "not_contains",
                "starts_with",
                "ends_with",
            ],
        }
    }
}

/// Filterable fields: system fields first, then content fields.
pub fn filterable_fields(ct: &ContentType) -> Vec<(String, FilterKind)> {
    let mut out = vec![
        ("id".to_string(), FilterKind::Id),
        ("createdAt".to_string(), FilterKind::DateTime),
        ("updatedAt".to_string(), FilterKind::DateTime),
        ("publishedAt".to_string(), FilterKind::DateTime),
        ("workflowStatus".to_string(), FilterKind::Enum),
    ];
    for f in &ct.fields {
        if let Some(kind) = FilterKind::of(&f.field_type) {
            out.push((f.name.clone(), kind));
        }
    }
    out
}

/// Sortable fields, used to generate the `XxxOrderByInput` enum.
pub fn sortable_fields(ct: &ContentType) -> Vec<String> {
    let mut out: Vec<String> = ["id", "createdAt", "updatedAt", "publishedAt"].map(String::from).into();
    out.extend(
        ct.fields
            .iter()
            .filter(|f| f.field_type.is_ordered() || f.field_type == FieldType::Boolean)
            .map(|f| f.name.clone()),
    );
    out
}

pub fn parse_where(ct: &ContentType, input: &Value) -> Result<Option<Filter>> {
    let Value::Object(obj) = input else {
        return Ok(None);
    };
    let kinds = filterable_fields(ct);
    let mut clauses = Vec::new();
    for (key, value) in obj {
        match key.as_str() {
            "AND" | "OR" | "NOT" => {
                let Value::Array(items) = value else { continue };
                let subs = items
                    .iter()
                    .map(|i| Ok(parse_where(ct, i)?.unwrap_or(Filter::And(vec![]))))
                    .collect::<Result<Vec<_>>>()?;
                clauses.push(match key.as_str() {
                    "AND" => Filter::And(subs),
                    "OR" => Filter::Or(subs),
                    _ => Filter::And(subs.into_iter().map(|f| Filter::Not(Box::new(f))).collect()),
                });
            }
            _ => {
                // Field names never contain `_`, so the first one splits off the operator.
                let (field, suffix) = key.split_once('_').unwrap_or((key.as_str(), ""));
                let kind = kinds
                    .iter()
                    .find(|(name, _)| name == field)
                    .map(|(_, k)| *k)
                    .ok_or_else(|| CmsError::validation(format!("cannot filter on `{key}`")))?;
                let op = if suffix.is_empty() {
                    Op::Eq
                } else {
                    SUFFIXES
                        .iter()
                        .find(|(s, _)| *s == suffix)
                        .map(|(_, op)| *op)
                        .ok_or_else(|| CmsError::validation(format!("unknown filter `{key}`")))?
                };
                if value.is_null() && op != Op::Eq && op != Op::Ne {
                    continue;
                }
                let value = normalize(kind, value)?;
                let cmp = Filter::cmp(field, op, value);
                clauses.push(if suffix == "not_contains" { Filter::Not(Box::new(cmp)) } else { cmp });
            }
        }
    }
    Ok(match clauses.len() {
        0 => None,
        1 => clauses.pop(),
        _ => Some(Filter::And(clauses)),
    })
}

/// Brings comparison values into stored form (canonical timestamps, dates
/// and lowercase UUIDs).
fn normalize(kind: FilterKind, value: &Value) -> Result<Value> {
    let parse: fn(&str) -> Option<String> = match kind {
        FilterKind::DateTime => |s| parse_ts(s).map(format_ts),
        FilterKind::Date => |s| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().map(|d| d.format("%Y-%m-%d").to_string())
        },
        FilterKind::Uuid => |s| uuid::Uuid::parse_str(s).ok().map(|u| u.to_string()),
        _ => return Ok(value.clone()),
    };
    let one = |v: &Value| -> Result<Value> {
        match v {
            Value::Null => Ok(Value::Null),
            Value::String(s) => parse(s)
                .map(Value::String)
                .ok_or_else(|| CmsError::validation(format!("invalid value `{s}` in filter"))),
            other => Err(CmsError::validation(format!("invalid value `{other}` in filter"))),
        }
    };
    match value {
        Value::Array(xs) => Ok(Value::Array(xs.iter().map(one).collect::<Result<_>>()?)),
        v => one(v),
    }
}

/// Rewrites the opaque API ids in a where input (`id*` and single-relation
/// filters) into storage UUIDs. `decode(token, expected_type)` must return the
/// UUID string or an error.
pub fn decode_where_ids(
    ct: &ContentType,
    input: &mut Value,
    decode: &dyn Fn(&str, &str) -> Result<String>,
) -> Result<()> {
    let Value::Object(obj) = input else {
        return Ok(());
    };
    for (key, value) in obj.iter_mut() {
        if matches!(key.as_str(), "AND" | "OR" | "NOT") {
            if let Value::Array(items) = value {
                for item in items {
                    decode_where_ids(ct, item, decode)?;
                }
            }
            continue;
        }
        let field = key.split_once('_').map_or(key.as_str(), |(f, _)| f);
        let expected = match ct.field(field).map(|f| &f.field_type) {
            _ if field == "id" => ct.api_id.as_str(),
            Some(FieldType::Relation { target, many: false }) => target.as_str(),
            _ => continue,
        };
        match value {
            Value::String(token) => *token = decode(token, expected)?,
            Value::Array(items) => {
                for item in items {
                    if let Value::String(token) = item {
                        *token = decode(token, expected)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn parse_order_by(input: &Value) -> Result<Vec<SortKey>> {
    let items = match input {
        Value::Null => return Ok(vec![]),
        Value::Array(xs) => xs.clone(),
        single => vec![single.clone()],
    };
    items
        .iter()
        .map(|v| {
            let s = v.as_str().unwrap_or_default();
            let (field, dir) = s
                .rsplit_once('_')
                .ok_or_else(|| CmsError::validation(format!("invalid orderBy `{s}`")))?;
            let direction = match dir {
                "ASC" => Direction::Asc,
                "DESC" => Direction::Desc,
                _ => return Err(CmsError::validation(format!("invalid orderBy `{s}`"))),
            };
            Ok(SortKey { field: field.to_string(), direction })
        })
        .collect()
}
