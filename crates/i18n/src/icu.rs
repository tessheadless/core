// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! The small part of ICU MessageFormat needed to convert plurals and
//! placeholders between formats.

/// A message that is a single top-level plural: `{count, plural, one {…} other {…}}`.
#[derive(Debug, Clone, PartialEq)]
pub struct Plural {
    pub variable: String,
    /// `(category, text)`: `zero`, `one`, `two`, `few`, `many`, `other`, or `=0`…
    pub forms: Vec<(String, String)>,
}

pub const CATEGORIES: &[&str] = &["zero", "one", "two", "few", "many", "other"];

/// Parses `text` as a plural message; `None` when it is anything else.
pub fn parse_plural(text: &str) -> Option<Plural> {
    let inner = text.trim().strip_prefix('{')?.strip_suffix('}')?;
    let mut parts = inner.splitn(3, ',');
    let variable = parts.next()?.trim();
    if parts.next()?.trim() != "plural" || variable.is_empty() || !variable.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        return None;
    }
    let mut rest = parts.next()?.trim();
    let mut forms = vec![];
    while !rest.is_empty() {
        let open = rest.find('{')?;
        let category = rest[..open].trim();
        if category.is_empty() || category.contains(char::is_whitespace) {
            return None;
        }
        let mut depth = 0;
        let mut end = None;
        for (i, c) in rest[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = end?;
        forms.push((category.to_string(), rest[open + 1..end].to_string()));
        rest = rest[end + 1..].trim_start();
    }
    forms.iter().any(|(c, _)| c == "other").then(|| Plural { variable: variable.to_string(), forms })
}

pub fn format_plural(variable: &str, forms: &[(String, String)]) -> String {
    let body: Vec<String> = forms.iter().map(|(c, t)| format!("{c} {{{t}}}")).collect();
    format!("{{{variable}, plural, {}}}", body.join(" "))
}

/// Index of the `}` closing the `{` at `open`.
fn closing(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_') && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// `{name}` → `{{name}}` for i18next. Only simple arguments change: the
/// bodies of `plural`/`select` options (`one {x}`) are messages, converted
/// recursively.
pub fn to_double_braces(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = closing(rest, open) else {
            out.push_str(&rest[open..]);
            return out;
        };
        let inner = &rest[open + 1..close];
        let mut parts = inner.splitn(3, ',');
        let (var, kind, options) = (parts.next().unwrap_or_default(), parts.next(), parts.next());
        if is_identifier(inner.trim()) {
            out.push_str(&format!("{{{{{}}}}}", inner.trim()));
        } else if let (Some(kind), Some(options)) = (kind, options)
            && matches!(kind.trim(), "plural" | "select" | "selectordinal")
        {
            out.push_str(&format!("{{{},{kind},", var));
            out.push_str(&convert_options(options));
            out.push('}');
        } else {
            out.push_str(&rest[open..=close]);
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// `one {msg} other {msg}`: selectors kept, messages converted.
fn convert_options(options: &str) -> String {
    let mut out = String::new();
    let mut rest = options;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = closing(rest, open) else {
            out.push_str(&rest[open..]);
            return out;
        };
        out.push('{');
        out.push_str(&to_double_braces(&rest[open + 1..close]));
        out.push('}');
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// `{{name}}` → `{name}`.
pub fn from_double_braces(text: &str) -> String {
    let re = regex::Regex::new(r"\{\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}\}").unwrap();
    re.replace_all(text, "{$1}").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plurals() {
        let p = parse_plural("{count, plural, one {# item} other {# items in {place}}}").unwrap();
        assert_eq!(p.variable, "count");
        assert_eq!(p.forms, vec![("one".into(), "# item".into()), ("other".into(), "# items in {place}".into())]);
        assert_eq!(format_plural(&p.variable, &p.forms), "{count, plural, one {# item} other {# items in {place}}}");
        assert!(parse_plural("Hello {name}").is_none());
        assert!(parse_plural("{n, plural, one {x}}").is_none(), "other is mandatory");
        assert!(parse_plural("{n, select, a {x} other {y}}").is_none());
        assert_eq!(to_double_braces("Hi {name}, {n, plural, one {x}}"), "Hi {{name}}, {n, plural, one {x}}");
        assert_eq!(
            to_double_braces("{g, select, male {He met {who}} other {They met {who}}}"),
            "{g, select, male {He met {{who}}} other {They met {{who}}}}"
        );
        assert_eq!(to_double_braces("{n, number} {broken"), "{n, number} {broken");
        assert_eq!(from_double_braces("Hi {{ name }}"), "Hi {name}");
    }
}
