// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! GraphQL objects backed by a `serde_json::Value` (camelCase records of the
//! built-in services), so their types read as plain field lists.

use async_graphql::dynamic::{Enum, Field, FieldFuture, FieldValue, Object, TypeRef};
use async_graphql::{Name, Value as GqlValue};
use serde_json::Value;

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    /// Scalar or list of scalars.
    Plain,
    /// Enum, or list of enums, stored as strings.
    Enum,
    /// Nested object, or list of objects.
    Object,
}

pub(crate) struct F {
    pub name: &'static str,
    pub ty: TypeRef,
    pub kind: Kind,
    pub description: Option<&'static str>,
}

pub(crate) fn plain(name: &'static str, ty: TypeRef) -> F {
    F {
        name,
        ty,
        kind: Kind::Plain,
        description: None,
    }
}

pub(crate) fn enumerated(name: &'static str, ty: TypeRef) -> F {
    F {
        name,
        ty,
        kind: Kind::Enum,
        description: None,
    }
}

pub(crate) fn object(name: &'static str, ty: TypeRef) -> F {
    F {
        name,
        ty,
        kind: Kind::Object,
        description: None,
    }
}

impl F {
    pub(crate) fn doc(mut self, d: &'static str) -> F {
        self.description = Some(d);
        self
    }
}

fn enum_value(v: &Value) -> Option<GqlValue> {
    match v {
        Value::String(s) => Some(GqlValue::Enum(Name::new(s))),
        Value::Array(xs) => Some(GqlValue::List(xs.iter().filter_map(enum_value).collect())),
        _ => None,
    }
}

pub(crate) fn value(v: Value) -> FieldValue<'static> {
    FieldValue::owned_any(v)
}

pub(crate) fn values(vs: impl IntoIterator<Item = Value>) -> FieldValue<'static> {
    FieldValue::list(vs.into_iter().map(value))
}

/// An object type whose fields are read from the parent JSON value.
pub(crate) fn json_object(type_name: &str, description: &str, fields: Vec<F>) -> Object {
    let mut obj = Object::new(type_name).description(description);
    for f in fields {
        let (name, kind) = (f.name, f.kind);
        let mut field = Field::new(name, f.ty, move |ctx| {
            FieldFuture::new(async move {
                let parent = ctx.parent_value.try_downcast_ref::<Value>()?;
                let v = parent.get(name).cloned().unwrap_or(Value::Null);
                if v.is_null() {
                    return Ok(None);
                }
                Ok(match kind {
                    Kind::Plain => Some(FieldValue::value(GqlValue::from_json(v)?)),
                    Kind::Enum => enum_value(&v).map(FieldValue::value),
                    Kind::Object => match v {
                        Value::Array(items) => Some(values(items)),
                        other => Some(value(other)),
                    },
                })
            })
        });
        if let Some(d) = f.description {
            field = field.description(d);
        }
        obj = obj.field(field);
    }
    obj
}

pub(crate) fn enum_type(name: &str, items: impl IntoIterator<Item = &'static str>) -> Enum {
    items.into_iter().fold(Enum::new(name), |e, i| e.item(i))
}
