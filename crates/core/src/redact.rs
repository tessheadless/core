// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Removes secret values (PASSWORD hashes) from entry data before it leaves
//! the server through outgoing integrations (webhooks).

use serde_json::{Map, Value};

use crate::model::{COMPONENT_KEY, FieldDef, FieldType};
use crate::validate::Components;

/// Drops every PASSWORD field of `data`, including inside components and
/// dynamic zones.
pub fn redact_secrets(fields: &[FieldDef], data: &mut Map<String, Value>, components: &Components) {
    for f in fields {
        match &f.field_type {
            t if t.is_secret() => {
                data.remove(&f.name);
            }
            FieldType::Component { component, .. } => {
                if let Some(c) = components.get(component) {
                    for_each_object(data.get_mut(&f.name), |o| {
                        redact_secrets(&c.fields, o, components)
                    });
                }
            }
            FieldType::DynamicZone { .. } => for_each_object(data.get_mut(&f.name), |o| {
                let name = o
                    .get(COMPONENT_KEY)
                    .and_then(Value::as_str)
                    .map(String::from);
                if let Some(c) = name.and_then(|n| components.get(&n)) {
                    redact_secrets(&c.fields, o, components);
                }
            }),
            _ => {}
        }
    }
}

fn for_each_object(v: Option<&mut Value>, mut f: impl FnMut(&mut Map<String, Value>)) {
    match v {
        Some(Value::Object(o)) => f(o),
        Some(Value::Array(xs)) => xs.iter_mut().filter_map(Value::as_object_mut).for_each(f),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::model::{Component, now};

    fn field(name: &str, field_type: FieldType) -> FieldDef {
        FieldDef {
            name: name.into(),
            field_type,
            required: false,
            description: None,
        }
    }

    #[test]
    fn strips_nested_passwords() {
        let pw = || FieldType::Password { min_length: 8 };
        let mut components = Components::new();
        components.insert(
            "Login".into(),
            Component {
                api_id: "Login".into(),
                description: None,
                fields: vec![field("user", FieldType::text()), field("pin", pw())],
                created_at: now(),
                updated_at: now(),
            },
        );
        let fields = vec![
            field("title", FieldType::text()),
            field("secret", pw()),
            field(
                "login",
                FieldType::Component {
                    component: "Login".into(),
                    repeatable: true,
                },
            ),
            field(
                "zone",
                FieldType::DynamicZone {
                    components: vec!["Login".into()],
                },
            ),
        ];
        let mut data = json!({
            "title": "t", "secret": "$argon2",
            "login": [{"user": "a", "pin": "$argon2"}],
            "zone": [{"__component": "Login", "user": "b", "pin": "$argon2"}],
        })
        .as_object()
        .unwrap()
        .clone();
        redact_secrets(&fields, &mut data, &components);
        assert_eq!(
            Value::Object(data),
            json!({"title": "t", "login": [{"user": "a"}], "zone": [{"__component": "Login", "user": "b"}]})
        );
    }
}
