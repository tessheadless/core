// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::{Map, Value, json};

use super::Message;
use crate::icu::{parse_plural, to_double_braces};
use crate::{Language, TranslationKey};

fn pretty(v: &Value) -> String {
    let mut s = serde_json::to_string_pretty(v).unwrap_or_default();
    s.push('\n');
    s
}

pub(super) fn json_flat(messages: &[Message]) -> String {
    let map: BTreeMap<&str, &str> = messages
        .iter()
        .map(|m| (m.key.name.as_str(), m.text.as_str()))
        .collect();
    pretty(&json!(map))
}

/// Inserts `value` at the dotted `path`. When a parent is already a string,
/// the rest of the path is kept as one literal key at that level.
fn insert_path(node: &mut Map<String, Value>, path: &str, value: Value) {
    let Some((head, rest)) = path.split_once('.') else {
        if node.get(path).is_some_and(Value::is_object) {
            // `a` after `a.b`: keep `a` under a key that cannot clash.
            insert_flat_conflict(node, path, value);
        } else {
            node.insert(path.to_string(), value);
        }
        return;
    };
    match node.get(head) {
        Some(v) if !v.is_object() => {
            node.insert(path.to_string(), value);
        }
        _ => {
            let child = node
                .entry(head.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            insert_path(child.as_object_mut().expect("object"), rest, value);
        }
    }
}

/// A leaf whose name is also a parent: stored as `name` inside the parent's
/// object is impossible, so it moves to the closest level as a dotted key.
fn insert_flat_conflict(node: &mut Map<String, Value>, name: &str, value: Value) {
    if let Some(Value::Object(child)) = node.get_mut(name) {
        child.insert(String::new(), value);
    }
}

/// i18next: plural keys become `key_one`, `key_other`, … and `{var}` becomes `{{var}}`.
pub(super) fn json_nested(messages: &[Message], i18next: bool) -> String {
    let mut root = Map::new();
    for m in messages {
        if !i18next {
            insert_path(&mut root, &m.key.name, json!(m.text));
            continue;
        }
        match parse_plural(&m.text).filter(|_| m.key.plural) {
            Some(p) => {
                for (category, text) in p.forms {
                    let text =
                        to_double_braces(&text).replace('#', &format!("{{{{{}}}}}", p.variable));
                    let suffix = category
                        .strip_prefix('=')
                        .map_or(category.clone(), String::from);
                    insert_path(&mut root, &format!("{}_{suffix}", m.key.name), json!(text));
                }
            }
            None => insert_path(&mut root, &m.key.name, json!(to_double_braces(&m.text))),
        }
    }
    pretty(&Value::Object(root))
}

pub(super) fn arb(messages: &[Message], lang: &str) -> String {
    let mut root = Map::new();
    root.insert("@@locale".into(), json!(super::underscored(lang)));
    for m in messages {
        root.insert(m.key.name.clone(), json!(m.text));
        if let Some(d) = &m.key.description {
            root.insert(format!("@{}", m.key.name), json!({ "description": d }));
        }
    }
    pretty(&Value::Object(root))
}

pub(crate) fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

pub(super) fn xliff(
    keys: &[&TranslationKey],
    ns: &str,
    source: &str,
    target: &str,
    texts: impl Fn(&TranslationKey) -> (Option<String>, Option<String>),
) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\">\n");
    let original = if ns.is_empty() { "default" } else { ns };
    let _ = writeln!(
        out,
        "  <file original=\"{}\" source-language=\"{}\" target-language=\"{}\" datatype=\"plaintext\">\n    <body>",
        xml_escape(original),
        xml_escape(source),
        xml_escape(target)
    );
    for k in keys {
        let (src, tgt) = texts(k);
        let name = xml_escape(&k.name);
        let _ = writeln!(out, "      <trans-unit id=\"{name}\" resname=\"{name}\">");
        let _ = writeln!(
            out,
            "        <source>{}</source>",
            xml_escape(src.as_deref().unwrap_or_default())
        );
        if let Some(t) = tgt {
            let _ = writeln!(
                out,
                "        <target state=\"translated\">{}</target>",
                xml_escape(&t)
            );
        }
        if let Some(d) = &k.description {
            let _ = writeln!(out, "        <note>{}</note>", xml_escape(d));
        }
        out.push_str("      </trans-unit>\n");
    }
    out.push_str("    </body>\n  </file>\n</xliff>\n");
    out
}

fn c_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

pub(super) fn po(messages: &[Message], lang: &str) -> String {
    let mut out = format!(
        "msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\"Language: {}\\n\"\n\"X-Generator: Tess\\n\"\n",
        super::underscored(lang)
    );
    for m in messages {
        out.push('\n');
        if let Some(d) = &m.key.description {
            for line in d.lines() {
                let _ = writeln!(out, "#. {line}");
            }
        }
        let _ = writeln!(
            out,
            "msgid \"{}\"\nmsgstr \"{}\"",
            c_escape(&m.key.name),
            c_escape(&m.text)
        );
    }
    out
}

fn properties_escape(s: &str, is_key: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '=' | ':' | '#' | '!' if is_key => {
                out.push('\\');
                out.push(c);
            }
            ' ' if is_key || i == 0 => out.push_str("\\ "),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out
}

pub(super) fn properties(messages: &[Message]) -> String {
    let mut out = String::new();
    for m in messages {
        if let Some(d) = &m.key.description {
            for line in d.lines() {
                let _ = writeln!(out, "# {line}");
            }
        }
        let _ = writeln!(
            out,
            "{}={}",
            properties_escape(&m.key.name, true),
            properties_escape(&m.text, false)
        );
    }
    out
}

/// Android resource names only allow `[a-z0-9_]`; other characters become `_`.
pub(crate) fn android_name(key: &str) -> String {
    let mut s: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

fn android_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("\\'"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '@' | '?' if i == 0 => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

pub(super) fn android(messages: &[Message]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n");
    for m in messages {
        let name = android_name(&m.key.name);
        if let Some(d) = &m.key.description {
            let _ = writeln!(out, "    <!-- {} -->", d.replace("--", "- -"));
        }
        match parse_plural(&m.text).filter(|_| m.key.plural) {
            Some(p) => {
                let _ = writeln!(out, "    <plurals name=\"{name}\">");
                for (category, text) in p
                    .forms
                    .iter()
                    .filter(|(c, _)| crate::icu::CATEGORIES.contains(&c.as_str()))
                {
                    let text = android_escape(text).replace('#', "%d");
                    let _ = writeln!(out, "        <item quantity=\"{category}\">{text}</item>");
                }
                out.push_str("    </plurals>\n");
            }
            None => {
                let _ = writeln!(
                    out,
                    "    <string name=\"{name}\">{}</string>",
                    android_escape(&m.text)
                );
            }
        }
    }
    out.push_str("</resources>\n");
    out
}

pub(super) fn apple(messages: &[Message]) -> String {
    let mut out = String::new();
    for m in messages {
        if let Some(d) = &m.key.description {
            let _ = writeln!(out, "/* {} */", d.replace("*/", "* /"));
        }
        let _ = writeln!(
            out,
            "\"{}\" = \"{}\";",
            c_escape(&m.key.name),
            c_escape(&m.text)
        );
    }
    out
}

fn yaml_scalar(s: &str) -> String {
    // A JSON string is a valid YAML double-quoted scalar.
    serde_json::to_string(s).unwrap_or_default()
}

fn yaml_map(out: &mut String, map: &Map<String, Value>, indent: usize) {
    for (k, v) in map {
        let pad = "  ".repeat(indent);
        match v {
            Value::Object(child) => {
                let _ = writeln!(out, "{pad}{}:", yaml_scalar(k));
                yaml_map(out, child, indent + 1);
            }
            Value::String(s) => {
                let _ = writeln!(out, "{pad}{}: {}", yaml_scalar(k), yaml_scalar(s));
            }
            _ => {}
        }
    }
}

pub(super) fn yaml(messages: &[Message], lang: &str) -> String {
    let mut tree = Map::new();
    for m in messages {
        insert_path(&mut tree, &m.key.name, json!(m.text));
    }
    let mut root = Map::new();
    root.insert(lang.to_string(), Value::Object(tree));
    let mut out = String::new();
    yaml_map(&mut out, &root, 0);
    out
}

pub(crate) fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) || s.starts_with(' ') || s.ends_with(' ') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub(super) fn csv(
    keys: &[&TranslationKey],
    langs: &[&Language],
    text: impl Fn(&TranslationKey, &str) -> Option<String>,
) -> String {
    let mut out = String::from("key,description");
    for l in langs {
        out.push(',');
        out.push_str(&csv_field(&l.tag));
    }
    out.push_str("\r\n");
    for k in keys {
        out.push_str(&csv_field(&k.name));
        out.push(',');
        out.push_str(&csv_field(k.description.as_deref().unwrap_or_default()));
        for l in langs {
            out.push(',');
            out.push_str(&csv_field(&text(k, &l.tag).unwrap_or_default()));
        }
        out.push_str("\r\n");
    }
    out
}
