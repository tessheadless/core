// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};
use tess_core::{CmsError, Result};

use crate::{FieldKind, FormField, HONEYPOT};

static SLUG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").unwrap());
static NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_]{0,63}$").unwrap());
static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$").unwrap());
static PHONE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\+?[0-9 ()./-]{3,32}$").unwrap());

const MAX_FIELDS: usize = 50;
const MAX_OPTIONS: usize = 100;
const MAX_TEXT: u32 = 1_000;
const MAX_TEXTAREA: u32 = 10_000;

fn bad(msg: impl Into<String>) -> CmsError {
    CmsError::validation(msg)
}

/// Checks a form definition: slug, field names, labels, options.
pub fn check_definition(slug: &str, title: &str, fields: &[FormField]) -> Result<()> {
    if slug.len() > 64 || !SLUG.is_match(slug) {
        return Err(bad(format!(
            "slug `{slug}`: use lowercase letters, digits and dashes (at most 64)"
        )));
    }
    if title.trim().is_empty() || title.len() > 200 {
        return Err(bad("the title is required (at most 200 characters)"));
    }
    if fields.is_empty() || fields.len() > MAX_FIELDS {
        return Err(bad(format!("a form has 1 to {MAX_FIELDS} fields")));
    }
    let mut names = HashSet::new();
    for f in fields {
        if !NAME.is_match(&f.name) || f.name == HONEYPOT {
            return Err(bad(format!(
                "field name `{}`: start with a letter, then letters, digits or `_`",
                f.name
            )));
        }
        if !names.insert(f.name.as_str()) {
            return Err(bad(format!("field `{}` appears twice", f.name)));
        }
        if f.label.trim().is_empty() || f.label.len() > 200 {
            return Err(bad(format!(
                "field `{}`: the label is required (at most 200 characters)",
                f.name
            )));
        }
        let choices = matches!(f.kind, FieldKind::Select | FieldKind::MultiSelect);
        if choices && (f.options.is_empty() || f.options.len() > MAX_OPTIONS) {
            return Err(bad(format!(
                "field `{}`: give 1 to {MAX_OPTIONS} options",
                f.name
            )));
        }
        if !choices && !f.options.is_empty() {
            return Err(bad(format!(
                "field `{}`: only SELECT and MULTI_SELECT fields have options",
                f.name
            )));
        }
        if f.options
            .iter()
            .any(|o| o.trim().is_empty() || o.len() > 200)
        {
            return Err(bad(format!(
                "field `{}`: options are 1 to 200 characters",
                f.name
            )));
        }
        if let (Some(min), Some(max)) = (f.min, f.max)
            && min > max
        {
            return Err(bad(format!("field `{}`: min is above max", f.name)));
        }
    }
    Ok(())
}

/// Text of a submitted value: strings as they are, numbers and booleans as text.
fn text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn empty(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.trim().is_empty(),
        Some(Value::Array(a)) => a.is_empty(),
        _ => false,
    }
}

fn check_field(f: &FormField, v: &Value) -> std::result::Result<Value, String> {
    let limit = |default: u32| f.max_length.unwrap_or(default) as usize;
    match f.kind {
        FieldKind::Text | FieldKind::Textarea => {
            let s = text(v).ok_or("must be text")?;
            let max = limit(if f.kind == FieldKind::Text {
                MAX_TEXT
            } else {
                MAX_TEXTAREA
            });
            if s.chars().count() > max {
                return Err(format!("at most {max} characters"));
            }
            Ok(Value::String(s))
        }
        FieldKind::Email => {
            let s = text(v).ok_or("must be an email address")?;
            (s.len() <= 254 && EMAIL.is_match(&s))
                .then_some(Value::String(s))
                .ok_or_else(|| "must be an email address".into())
        }
        FieldKind::Phone => {
            let s = text(v).ok_or("must be a phone number")?;
            PHONE
                .is_match(&s)
                .then_some(Value::String(s))
                .ok_or_else(|| "must be a phone number".into())
        }
        FieldKind::Url => {
            let s = text(v).ok_or("must be a URL")?;
            ((s.starts_with("https://") || s.starts_with("http://"))
                && s.len() <= 2_000
                && !s.contains(char::is_whitespace))
            .then_some(Value::String(s))
            .ok_or_else(|| "must be an http(s) URL".into())
        }
        FieldKind::Number => {
            let n: f64 = match v {
                Value::Number(n) => n.as_f64().ok_or("must be a number")?,
                Value::String(s) => s
                    .trim()
                    .replace(',', ".")
                    .parse()
                    .map_err(|_| "must be a number")?,
                _ => return Err("must be a number".into()),
            };
            if !n.is_finite() {
                return Err("must be a number".into());
            }
            if f.min.is_some_and(|min| n < min) || f.max.is_some_and(|max| n > max) {
                return Err(match (f.min, f.max) {
                    (Some(a), Some(b)) => format!("must be between {a} and {b}"),
                    (Some(a), None) => format!("must be at least {a}"),
                    (_, _) => format!("must be at most {}", f.max.unwrap_or_default()),
                });
            }
            Ok(serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number))
        }
        FieldKind::Date => {
            let s = text(v).ok_or("must be a date (YYYY-MM-DD)")?;
            chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                .map(|_| Value::String(s))
                .map_err(|_| "must be a date (YYYY-MM-DD)".into())
        }
        FieldKind::Datetime => {
            let s = text(v).ok_or("must be a date and time (YYYY-MM-DDTHH:MM)")?;
            let ok = ["%Y-%m-%dT%H:%M", "%Y-%m-%dT%H:%M:%S"]
                .iter()
                .any(|f| chrono::NaiveDateTime::parse_from_str(&s, f).is_ok());
            ok.then_some(Value::String(s))
                .ok_or_else(|| "must be a date and time (YYYY-MM-DDTHH:MM)".into())
        }
        FieldKind::Select => {
            let s = text(v).ok_or("must be one of the options")?;
            f.options
                .contains(&s)
                .then_some(Value::String(s))
                .ok_or_else(|| "must be one of the options".into())
        }
        FieldKind::MultiSelect => {
            let items: Vec<String> = match v {
                Value::Array(a) => a
                    .iter()
                    .map(|x| text(x).ok_or("must be a list of options"))
                    .collect::<std::result::Result<_, _>>()?,
                other => vec![text(other).ok_or("must be a list of options")?],
            };
            if items.iter().any(|i| !f.options.contains(i)) {
                return Err("must be among the options".into());
            }
            Ok(Value::Array(items.into_iter().map(Value::String).collect()))
        }
        FieldKind::Checkbox => match v {
            Value::Bool(b) => Ok(Value::Bool(*b)),
            Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
                "on" | "true" | "yes" | "1" => Ok(Value::Bool(true)),
                "" | "off" | "false" | "no" | "0" => Ok(Value::Bool(false)),
                _ => Err("must be true or false".into()),
            },
            _ => Err("must be true or false".into()),
        },
    }
}

/// Validates submitted values against the form's fields. Returns the clean
/// values (unknown keys dropped), or an error message per field.
pub fn validate_submission(
    fields: &[FormField],
    raw: &Map<String, Value>,
) -> std::result::Result<Map<String, Value>, BTreeMap<String, String>> {
    let mut data = Map::new();
    let mut errors = BTreeMap::new();
    for f in fields {
        let value = raw.get(&f.name);
        // A required checkbox must be checked (e.g. "I accept the terms").
        let missing = empty(value)
            || (f.kind == FieldKind::Checkbox
                && f.required
                && matches!(
                    value.map(|v| check_field(f, v)),
                    Some(Ok(Value::Bool(false)))
                ));
        if missing {
            if f.required {
                errors.insert(f.name.clone(), "is required".into());
            } else if f.kind == FieldKind::Checkbox {
                data.insert(f.name.clone(), Value::Bool(false));
            }
            continue;
        }
        match check_field(f, value.expect("not empty")) {
            Ok(v) => {
                data.insert(f.name.clone(), v);
            }
            Err(e) => {
                errors.insert(f.name.clone(), e);
            }
        }
    }
    if errors.is_empty() {
        Ok(data)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn field(name: &str, kind: FieldKind, required: bool) -> FormField {
        FormField {
            name: name.into(),
            label: name.into(),
            kind,
            required,
            help: None,
            placeholder: None,
            options: vec![],
            max_length: None,
            min: None,
            max: None,
        }
    }

    fn fields() -> Vec<FormField> {
        let mut seats = field("seats", FieldKind::Number, true);
        seats.min = Some(1.0);
        seats.max = Some(8.0);
        let mut room = field("room", FieldKind::Select, false);
        room.options = vec!["Terrace".into(), "Inside".into()];
        let mut extras = field("extras", FieldKind::MultiSelect, false);
        extras.options = vec!["Cake".into(), "Wine".into()];
        vec![
            field("name", FieldKind::Text, true),
            field("email", FieldKind::Email, true),
            seats,
            field("day", FieldKind::Date, true),
            field("slot", FieldKind::Datetime, false),
            room,
            extras,
            field("terms", FieldKind::Checkbox, true),
            field("news", FieldKind::Checkbox, false),
            field("site", FieldKind::Url, false),
            field("phone", FieldKind::Phone, false),
        ]
    }

    #[test]
    fn valid_submissions_are_normalized() {
        let raw = json!({"name": " Ada ", "email": "ada@example.com", "seats": "4", "day": "2026-12-24",
            "slot": "2026-12-24T19:30", "room": "Terrace", "extras": "Cake", "terms": "on", "phone": "+33 6 12 34 56 78",
            "unknown": "dropped"});
        let data = validate_submission(&fields(), raw.as_object().unwrap()).unwrap();
        assert_eq!(data["name"], "Ada");
        assert_eq!(data["seats"], 4.0);
        assert_eq!(data["extras"], json!(["Cake"]));
        assert_eq!(data["terms"], true);
        assert_eq!(data["news"], false, "unchecked optional checkbox");
        assert!(!data.contains_key("unknown") && !data.contains_key("site"));
    }

    #[test]
    fn errors_are_reported_per_field() {
        let raw = json!({"email": "nope", "seats": 12, "day": "24/12/2026", "room": "Roof", "extras": ["Cake", "Car"],
            "terms": false, "site": "javascript:alert(1)", "phone": "call me"});
        let errors = validate_submission(&fields(), raw.as_object().unwrap()).unwrap_err();
        assert_eq!(errors["name"], "is required");
        assert_eq!(errors["email"], "must be an email address");
        assert_eq!(errors["seats"], "must be between 1 and 8");
        assert_eq!(errors["day"], "must be a date (YYYY-MM-DD)");
        assert_eq!(errors["room"], "must be one of the options");
        assert_eq!(errors["extras"], "must be among the options");
        assert_eq!(
            errors["terms"], "is required",
            "a required checkbox must be checked"
        );
        assert_eq!(errors["site"], "must be an http(s) URL");
        assert_eq!(errors["phone"], "must be a phone number");
    }

    #[test]
    fn definitions_are_checked() {
        assert!(check_definition("contact", "Contact", &fields()).is_ok());
        assert!(check_definition("Contact Us", "Contact", &fields()).is_err());
        assert!(check_definition("contact", " ", &fields()).is_err());
        assert!(check_definition("contact", "Contact", &[]).is_err());
        let mut twice = fields();
        twice.push(field("name", FieldKind::Text, false));
        assert!(check_definition("contact", "Contact", &twice).is_err());
        let mut no_options = fields();
        no_options[5].options.clear();
        assert!(check_definition("contact", "Contact", &no_options).is_err());
        assert!(
            check_definition(
                "contact",
                "Contact",
                &[field("_hp", FieldKind::Text, false)]
            )
            .is_err()
        );
    }
}
