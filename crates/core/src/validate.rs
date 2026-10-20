// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::{Regex, RegexBuilder};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::error::{CmsError, Result};
use crate::model::{
    COMPONENT_KEY, Component, ComponentDef, ContentType, ContentTypeDef, FieldDef, FieldType, SYSTEM_FIELDS,
    format_ts, parse_ts,
};

static TYPE_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[A-Z][A-Za-z0-9]{0,63}$").unwrap());
static FIELD_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[a-z][A-Za-z0-9]{0,63}$").unwrap());
static ENUM_VALUE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^[A-Za-z_][A-Za-z0-9_]{0,63}$").unwrap());

/// Field names that clash with system fields or GraphQL conventions.
const RESERVED_FIELDS: &[&str] = &["ref", "stage", "documentInStages", "where", "orderBy"];
/// Components can nest, up to this depth.
pub const MAX_COMPONENT_DEPTH: usize = 5;
const MAX_PATTERN_LEN: usize = 500;
const MAX_PASSWORD_LEN: usize = 1024;

/// All components, by apiId.
pub type Components = BTreeMap<String, Component>;

/// Compiles a TEXT format: the whole value must match.
pub fn compile_pattern(pattern: &str) -> Result<Regex> {
    if pattern.len() > MAX_PATTERN_LEN {
        return Err(CmsError::validation(format!("patterns are limited to {MAX_PATTERN_LEN} characters")));
    }
    RegexBuilder::new(&format!("^(?:{pattern})$"))
        .size_limit(1 << 20)
        .build()
        .map_err(|e| CmsError::validation(format!("invalid pattern: {e}")))
}

fn type_name(api_id: &str, what: &str) -> Result<()> {
    if TYPE_NAME.is_match(api_id) {
        Ok(())
    } else {
        Err(CmsError::validation(format!(
            "{what} apiId `{api_id}` must be PascalCase alphanumeric (e.g. `BlogPost`), at most 64 chars"
        )))
    }
}

/// What field definitions may refer to.
struct DefCtx<'a> {
    /// Content types available as relation targets (incl. the one being defined).
    content_types: &'a BTreeSet<String>,
    components: &'a Components,
    /// Defining a component: no dynamic zones and no passwords.
    in_component: bool,
}

fn validate_fields(fields: &[FieldDef], ctx: &DefCtx<'_>) -> Result<()> {
    if fields.is_empty() {
        return Err(CmsError::validation("at least one field is needed"));
    }
    let mut seen = HashSet::new();
    for field in fields {
        let name = field.name.as_str();
        if !FIELD_NAME.is_match(name) {
            return Err(CmsError::validation(format!("field `{name}` must be camelCase alphanumeric, at most 64 chars")));
        }
        if SYSTEM_FIELDS.contains(&name) || RESERVED_FIELDS.contains(&name) {
            return Err(CmsError::validation(format!("field name `{name}` is reserved")));
        }
        if !seen.insert(name) {
            return Err(CmsError::validation(format!("duplicate field `{name}`")));
        }
        let bad = |msg: String| Err(CmsError::validation(format!("field `{name}`: {msg}")));
        match &field.field_type {
            FieldType::Text { min_length, max_length, pattern } => {
                if max_length == &Some(0) {
                    return bad("maxLength must be at least 1".into());
                }
                if let (Some(min), Some(max)) = (min_length, max_length)
                    && min > max
                {
                    return bad("minLength is greater than maxLength".into());
                }
                if let Some(Err(e)) = pattern.as_deref().map(compile_pattern) {
                    return bad(e.to_string().replace("validation error: ", ""));
                }
            }
            FieldType::Password { min_length } => {
                if ctx.in_component {
                    return bad("passwords are only allowed directly in content types".into());
                }
                if *min_length == 0 || *min_length as usize > MAX_PASSWORD_LEN {
                    return bad(format!("minLength must be between 1 and {MAX_PASSWORD_LEN}"));
                }
            }
            FieldType::Enum { values } => {
                if values.is_empty() {
                    return bad("an enumeration needs at least one value".into());
                }
                let mut seen_values = HashSet::new();
                for v in values {
                    if !ENUM_VALUE.is_match(v) || !seen_values.insert(v) {
                        return bad(format!("invalid or duplicate enum value `{v}`"));
                    }
                }
            }
            FieldType::Relation { target, .. } if !ctx.content_types.contains(target) => {
                return bad(format!("relation targets unknown content type `{target}`"));
            }
            FieldType::Component { component, .. } if !ctx.components.contains_key(component) => {
                return bad(format!("unknown component `{component}`"));
            }
            FieldType::DynamicZone { components } => {
                if ctx.in_component {
                    return bad("dynamic zones are not allowed inside components".into());
                }
                if components.is_empty() {
                    return bad("a dynamic zone needs at least one component".into());
                }
                let mut seen_components = HashSet::new();
                for c in components {
                    if !ctx.components.contains_key(c) {
                        return bad(format!("unknown component `{c}`"));
                    }
                    if !seen_components.insert(c) {
                        return bad(format!("component `{c}` is listed twice"));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Validates a content type definition against the other existing types and
/// the components. `others` must not contain the type being created/updated.
pub fn validate_content_type(def: &ContentTypeDef, others: &[ContentType], components: &Components) -> Result<String> {
    type_name(&def.api_id, "content type")?;
    if components.contains_key(&def.api_id) {
        return Err(CmsError::Conflict(format!("`{}` is already the name of a component", def.api_id)));
    }
    let singular = crate::model::lower_first(&def.api_id);
    let plural = def.plural_api_id.clone().unwrap_or_else(|| format!("{singular}s"));
    if !FIELD_NAME.is_match(&plural) {
        return Err(CmsError::validation(format!(
            "pluralApiId `{plural}` must be camelCase alphanumeric (e.g. `blogPosts`)"
        )));
    }
    if plural == singular {
        return Err(CmsError::validation("pluralApiId must differ from the singular form"));
    }
    for other in others {
        let other_names = [other.singular_api_id(), other.plural_api_id.clone()];
        if other_names.contains(&singular) || other_names.contains(&plural) {
            return Err(CmsError::Conflict(format!(
                "`{}` clashes with the API names of content type `{}`",
                def.api_id, other.api_id
            )));
        }
    }
    let mut targets: BTreeSet<String> = others.iter().map(|o| o.api_id.clone()).collect();
    targets.insert(def.api_id.clone());
    validate_fields(&def.fields, &DefCtx { content_types: &targets, components, in_component: false })?;
    Ok(plural)
}

/// Validates a component against the content types and the other components
/// (`others` must not contain the component being created/updated): fields,
/// no cycles through nested components, nesting depth.
pub fn validate_component(def: &ComponentDef, others: &Components, content_types: &[ContentType]) -> Result<()> {
    type_name(&def.api_id, "component")?;
    if content_types.iter().any(|c| c.api_id == def.api_id) {
        return Err(CmsError::Conflict(format!("`{}` is already the name of a content type", def.api_id)));
    }
    let mut all = others.clone();
    all.insert(
        def.api_id.clone(),
        Component {
            api_id: def.api_id.clone(),
            description: def.description.clone(),
            fields: def.fields.clone(),
            created_at: crate::model::now(),
            updated_at: crate::model::now(),
        },
    );
    let targets: BTreeSet<String> = content_types.iter().map(|c| c.api_id.clone()).collect();
    validate_fields(&def.fields, &DefCtx { content_types: &targets, components: &all, in_component: true })?;
    // Depth-first walk from the new definition: a cycle or excessive depth fails.
    fn walk(c: &str, all: &Components, path: &mut Vec<String>) -> Result<()> {
        if path.iter().any(|p| p == c) {
            path.push(c.to_string());
            return Err(CmsError::validation(format!("components cannot contain themselves: {}", path.join(" → "))));
        }
        if path.len() >= MAX_COMPONENT_DEPTH {
            return Err(CmsError::validation(format!("components can be nested at most {MAX_COMPONENT_DEPTH} deep")));
        }
        path.push(c.to_string());
        for f in all.get(c).map(|c| c.fields.as_slice()).unwrap_or_default() {
            if let FieldType::Component { component, .. } = &f.field_type {
                walk(component, all, path)?;
            }
        }
        path.pop();
        Ok(())
    }
    walk(&def.api_id, &all, &mut vec![])?;
    // Content types and components already using it must stay within the depth limit.
    for user in all.values() {
        walk(&user.api_id, &all, &mut vec![])?;
    }
    Ok(())
}

// ---- data ---------------------------------------------------------------------

/// Type-checks and normalizes entry data against its content type. `null`
/// values are dropped (a missing field and a null field are the same thing).
/// Password fields must already be hashed (see [`crate::password`]).
pub fn normalize_entry_data(
    ct: &ContentType,
    data: Map<String, Value>,
    components: &Components,
) -> Result<Map<String, Value>> {
    normalize_fields(&ct.fields, &ct.api_id, "", data, components, 0)
}

fn normalize_fields(
    fields: &[FieldDef],
    owner: &str,
    path: &str,
    data: Map<String, Value>,
    components: &Components,
    depth: usize,
) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    for (key, value) in data {
        let at = format!("{path}{key}");
        let field = fields
            .iter()
            .find(|f| f.name == key)
            .ok_or_else(|| CmsError::validation(format!("unknown field `{at}` on `{owner}`")))?;
        if value.is_null() {
            continue;
        }
        let normalized = normalize_value(&field.field_type, &at, value, components, depth)?;
        out.insert(key, normalized);
    }
    for field in fields {
        if field.required && !out.contains_key(&field.name) {
            return Err(CmsError::validation(format!("field `{path}{}` is required", field.name)));
        }
    }
    Ok(out)
}

fn normalize_value(ft: &FieldType, at: &str, value: Value, components: &Components, depth: usize) -> Result<Value> {
    let bad = |expected: &str| CmsError::validation(format!("field `{at}` expects {expected}, got {value}"));
    Ok(match ft {
        FieldType::Text { min_length, max_length, pattern } => {
            let s = value.as_str().ok_or_else(|| bad("a string"))?;
            let len = s.chars().count();
            if let Some(min) = min_length
                && len < *min as usize
            {
                return Err(CmsError::validation(format!("field `{at}` needs at least {min} characters")));
            }
            if let Some(max) = max_length
                && len > *max as usize
            {
                return Err(CmsError::validation(format!("field `{at}` is limited to {max} characters")));
            }
            if let Some(p) = pattern
                && !compile_pattern(p)?.is_match(s)
            {
                return Err(CmsError::validation(format!("field `{at}` does not match the expected format")));
            }
            value
        }
        FieldType::Markdown => {
            let s = value.as_str().ok_or_else(|| bad("a Markdown string"))?;
            if let Some(tag) = find_html(s) {
                return Err(CmsError::validation(format!(
                    "field `{at}` contains HTML (`{tag}`): use Markdown, or MDX components (capitalized, e.g. `<Callout>`)"
                )));
            }
            value
        }
        FieldType::Int => value.is_i64().then(|| value.clone()).ok_or_else(|| bad("an integer"))?,
        FieldType::Float => value.is_number().then(|| value.clone()).ok_or_else(|| bad("a number"))?,
        FieldType::Boolean => value.is_boolean().then(|| value.clone()).ok_or_else(|| bad("a boolean"))?,
        FieldType::Json => value,
        FieldType::DateTime => value
            .as_str()
            .and_then(parse_ts)
            .map(|ts| Value::String(format_ts(ts)))
            .ok_or_else(|| bad("an RFC 3339 date-time"))?,
        FieldType::Date => value
            .as_str()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .map(|d| Value::String(d.format("%Y-%m-%d").to_string()))
            .ok_or_else(|| bad("a date (YYYY-MM-DD)"))?,
        FieldType::Uuid => Value::String(parse_id(&value).ok_or_else(|| bad("a UUID"))?.to_string()),
        FieldType::Password { .. } => {
            // Hashed by the service before validation; anything else is a bug.
            let s = value.as_str().filter(|s| s.starts_with("$argon2")).ok_or_else(|| bad("a password"))?;
            Value::String(s.to_string())
        }
        FieldType::Enum { values } => value
            .as_str()
            .filter(|s| values.iter().any(|v| v == s))
            .map(|_| value.clone())
            .ok_or_else(|| bad(&format!("one of {values:?}")))?,
        FieldType::Relation { many: false, .. } => {
            Value::String(parse_id(&value).ok_or_else(|| bad("an entry id"))?.to_string())
        }
        FieldType::Relation { many: true, .. } => {
            let ids = value.as_array().ok_or_else(|| bad("a list of entry ids"))?;
            let mut unique = Vec::with_capacity(ids.len());
            for id in ids {
                let id = parse_id(id).ok_or_else(|| bad("a list of entry ids"))?.to_string();
                if !unique.contains(&id) {
                    unique.push(id);
                }
            }
            Value::Array(unique.into_iter().map(Value::String).collect())
        }
        FieldType::Component { component, repeatable } => {
            let def = components
                .get(component)
                .ok_or_else(|| CmsError::validation(format!("field `{at}` uses unknown component `{component}`")))?;
            let one = |v: Value, at: String| -> Result<Value> {
                let Value::Object(map) = v else {
                    return Err(CmsError::validation(format!("field `{at}` expects a `{component}` object")));
                };
                Ok(Value::Object(normalize_fields(&def.fields, component, &format!("{at}."), map, components, depth + 1)?))
            };
            if *repeatable {
                let Value::Array(items) = value else { return Err(bad(&format!("a list of `{component}`"))) };
                Value::Array(items.into_iter().enumerate().map(|(i, v)| one(v, format!("{at}[{i}]"))).collect::<Result<_>>()?)
            } else {
                one(value, at.to_string())?
            }
        }
        FieldType::DynamicZone { components: allowed } => {
            let Value::Array(items) = value else { return Err(bad("a list of components")) };
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.into_iter().enumerate() {
                let at_i = format!("{at}[{i}]");
                let Value::Object(mut map) = item else {
                    return Err(CmsError::validation(format!("field `{at_i}` expects a component object")));
                };
                let name = match map.remove(COMPONENT_KEY) {
                    Some(Value::String(s)) => s,
                    _ => return Err(CmsError::validation(format!("field `{at_i}` needs `{COMPONENT_KEY}`"))),
                };
                if !allowed.contains(&name) {
                    return Err(CmsError::validation(format!(
                        "field `{at_i}`: component `{name}` is not allowed here (allowed: {})",
                        allowed.join(", ")
                    )));
                }
                let def = components
                    .get(&name)
                    .ok_or_else(|| CmsError::validation(format!("field `{at_i}` uses unknown component `{name}`")))?;
                let mut normalized = normalize_fields(&def.fields, &name, &format!("{at_i}."), map, components, depth + 1)?;
                normalized.insert(COMPONENT_KEY.into(), Value::String(name));
                out.push(Value::Object(normalized));
            }
            Value::Array(out)
        }
    })
}

/// All `(target content type, entry id)` pairs referenced by relation fields,
/// including inside components and dynamic zones.
pub fn relation_refs(ct: &ContentType, data: &Map<String, Value>, components: &Components) -> Vec<(String, Uuid)> {
    let mut refs = Vec::new();
    collect_refs(&ct.fields, data, components, &mut refs);
    refs
}

fn collect_refs(fields: &[FieldDef], data: &Map<String, Value>, components: &Components, refs: &mut Vec<(String, Uuid)>) {
    for field in fields {
        let Some(value) = data.get(&field.name) else { continue };
        match &field.field_type {
            FieldType::Relation { target, .. } => {
                let ids: Vec<Uuid> = match value {
                    Value::Array(xs) => xs.iter().filter_map(parse_id).collect(),
                    v => parse_id(v).into_iter().collect(),
                };
                refs.extend(ids.into_iter().map(|id| (target.clone(), id)));
            }
            FieldType::Component { component, .. } => {
                if let Some(def) = components.get(component) {
                    for item in one_or_many(value) {
                        collect_refs(&def.fields, item, components, refs);
                    }
                }
            }
            FieldType::DynamicZone { .. } => {
                for item in one_or_many(value) {
                    if let Some(def) = item.get(COMPONENT_KEY).and_then(Value::as_str).and_then(|c| components.get(c)) {
                        collect_refs(&def.fields, item, components, refs);
                    }
                }
            }
            _ => {}
        }
    }
}

fn one_or_many(value: &Value) -> Vec<&Map<String, Value>> {
    match value {
        Value::Object(m) => vec![m],
        Value::Array(xs) => xs.iter().filter_map(Value::as_object).collect(),
        _ => vec![],
    }
}

fn parse_id(v: &Value) -> Option<Uuid> {
    v.as_str().and_then(|s| Uuid::parse_str(s).ok())
}

/// Finds raw HTML in Markdown/MDX, ignoring code blocks and code spans.
/// MDX stays allowed: JSX components start with an uppercase letter
/// (`<Callout>`, `</Callout>`), fragments are `<>`, expressions are `{…}`.
/// Autolinks (`<https://…>`, `<ada@example.com>`) are Markdown, not HTML.
pub fn find_html(markdown: &str) -> Option<String> {
    let mut fence: Option<(char, usize)> = None;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        let fence_char = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        if let Some(c) = fence_char {
            let run = trimmed.chars().take_while(|x| *x == c).count();
            if run >= 3 {
                match fence {
                    None => fence = Some((c, run)),
                    Some((fc, fr)) if fc == c && run >= fr && trimmed[run..].trim().is_empty() => fence = None,
                    _ => {}
                }
                continue;
            }
        }
        if fence.is_some() {
            continue;
        }
        if let Some(tag) = html_in_line(line) {
            return Some(tag);
        }
    }
    None
}

fn html_in_line(line: &str) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2, // escaped character
            '`' => {
                // Skip a code span: the next run of the same length closes it.
                let run = chars[i..].iter().take_while(|c| **c == '`').count();
                let mut j = i + run;
                let mut closed = None;
                while j < chars.len() {
                    if chars[j] == '`' {
                        let r = chars[j..].iter().take_while(|c| **c == '`').count();
                        if r == run {
                            closed = Some(j + r);
                            break;
                        }
                        j += r;
                    } else {
                        j += 1;
                    }
                }
                i = closed.unwrap_or(i + run);
            }
            '<' => {
                let rest: String = chars[i + 1..].iter().collect();
                if rest.starts_with('!') || rest.starts_with('?') {
                    return Some(format!("<{}", rest.chars().take(8).collect::<String>()));
                }
                let name_start = usize::from(rest.starts_with('/'));
                let name: String = rest[name_start..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
                let after = rest[name_start + name.len()..].chars().next();
                let is_tag = name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                    && matches!(after, None | Some(' ' | '\t' | '>' | '/'));
                if is_tag {
                    return Some(format!("<{}{name}>", if name_start == 1 { "/" } else { "" }));
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_html_detection() {
        for ok in [
            "# Title\n\nSome *text* with a [link](https://example.com).",
            "<Callout type=\"info\">MDX component</Callout>",
            "<Chart data={[1, 2, 3]} />\n\n{props.title}",
            "Autolinks: <https://example.com> and <ada@example.com>",
            "Inline code: `<div>` is fine, ``<span>`` too",
            "```html\n<div class=\"x\">code block</div>\n```",
            "~~~\n<script>alert(1)</script>\n~~~",
            "Escaped \\<div>, maths: a < b and 3<4, fragment <>x</>",
        ] {
            assert_eq!(find_html(ok), None, "{ok}");
        }
        for (bad, tag) in [
            ("Hello <div>world</div>", "<div>"),
            ("<script>alert(1)</script>", "<script>"),
            ("text </p>", "</p>"),
            ("<!-- comment -->", "<!-- comm"),
            ("<img src=x onerror=alert(1)>", "<img>"),
            ("<br/>", "<br>"),
            ("```\ncode\n```\n<iframe src=x>", "<iframe>"),
        ] {
            assert_eq!(find_html(bad).as_deref(), Some(tag), "{bad}");
        }
    }

    #[test]
    fn patterns_match_the_whole_value() {
        let email = compile_pattern(r"[^@\s]+@[^@\s]+\.[a-z]{2,}").unwrap();
        assert!(email.is_match("ada@example.com"));
        assert!(!email.is_match("ada@example.com and more"));
        assert!(compile_pattern("(unclosed").is_err());
        assert!(compile_pattern(&"a".repeat(501)).is_err());
    }
}
