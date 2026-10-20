// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use quick_xml::events::{BytesRef, BytesStart, Event};
use regex::Regex;
use serde_json::{Map, Value};
use tess_core::{CmsError, Result};

use super::Format;
use crate::icu::{CATEGORIES, format_plural, from_double_braces};
use crate::normalize_tag;

/// A key read from a file, with its texts by language.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportedKey {
    pub name: String,
    pub description: Option<String>,
    pub plural: bool,
    /// `(language tag, text)`.
    pub translations: Vec<(String, String)>,
}

fn bad(format: Format, msg: impl std::fmt::Display) -> CmsError {
    CmsError::validation(format!("invalid {} file: {msg}", format.as_str()))
}

/// Keeps keys in file order, merging texts of the same key.
#[derive(Default)]
struct Collector {
    order: Vec<String>,
    keys: BTreeMap<String, ImportedKey>,
}

impl Collector {
    fn entry(&mut self, name: &str) -> &mut ImportedKey {
        if !self.keys.contains_key(name) {
            self.order.push(name.to_string());
        }
        self.keys.entry(name.to_string()).or_insert_with(|| ImportedKey { name: name.to_string(), ..Default::default() })
    }

    fn text(&mut self, name: &str, lang: &str, text: String) {
        let k = self.entry(name);
        k.translations.retain(|(l, _)| l != lang);
        k.translations.push((lang.to_string(), text));
    }

    fn finish(mut self) -> Vec<ImportedKey> {
        self.order.iter().filter_map(|n| self.keys.remove(n)).filter(|k| !k.name.is_empty()).collect()
    }
}

/// Parses an uploaded file. Single-language formats need `language`, unless
/// the file declares it (XLIFF `target-language`, ARB `@@locale`, PO `Language`).
pub fn parse(format: Format, content: &str, language: Option<&str>) -> Result<Vec<ImportedKey>> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let language = language.map(normalize_tag).transpose()?;
    let need_lang = |declared: Option<String>| -> Result<String> {
        match language.clone().or(declared.map(|d| normalize_tag(&d)).transpose()?) {
            Some(l) => Ok(l),
            None => Err(CmsError::validation(format!("{}: choose the language of the file", format.as_str()))),
        }
    };
    match format {
        Format::Json | Format::JsonNested => json(content, &need_lang(None)?, false),
        Format::I18next => json(content, &need_lang(None)?, true),
        Format::Arb => arb(content, need_lang),
        Format::Po => po(content, need_lang),
        Format::Properties => properties(content, &need_lang(None)?),
        Format::AndroidXml => android(content, &need_lang(None)?),
        Format::AppleStrings => apple(content, &need_lang(None)?),
        Format::Xliff => xliff(content, language.as_deref()),
        Format::Csv => csv(content),
        Format::Yaml => Err(CmsError::validation("YAML files can be exported but not imported")),
    }
}

// ---- JSON family -----------------------------------------------------------

fn flatten(prefix: &str, v: &Value, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(map) => {
            for (k, child) in map {
                // `""` holds the text of a key that is also a parent (see the exporter).
                let path = match (prefix.is_empty(), k.is_empty()) {
                    (true, _) => k.clone(),
                    (false, true) => prefix.to_string(),
                    (false, false) => format!("{prefix}.{k}"),
                };
                flatten(&path, child, out);
            }
        }
        Value::String(s) => out.push((prefix.to_string(), s.clone())),
        Value::Number(n) => out.push((prefix.to_string(), n.to_string())),
        Value::Bool(b) => out.push((prefix.to_string(), b.to_string())),
        Value::Null | Value::Array(_) => {}
    }
}

fn json(content: &str, lang: &str, i18next: bool) -> Result<Vec<ImportedKey>> {
    let v: Value = serde_json::from_str(content).map_err(|e| bad(Format::Json, e))?;
    if !v.is_object() {
        return Err(bad(Format::Json, "the top level must be an object"));
    }
    let mut pairs = vec![];
    flatten("", &v, &mut pairs);
    let mut c = Collector::default();
    if !i18next {
        for (k, t) in pairs {
            c.text(&k, lang, t);
        }
        return Ok(c.finish());
    }
    // i18next plurals: `key_one`, `key_other`, … → one ICU plural message.
    let names: std::collections::HashSet<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
    let plural_base = |k: &str| -> Option<(String, String)> {
        let (base, suffix) = k.rsplit_once('_')?;
        let category = if suffix.chars().all(|c| c.is_ascii_digit()) { format!("={suffix}") } else { suffix.to_string() };
        let known = CATEGORIES.contains(&suffix) || category.starts_with('=');
        (known && names.contains(format!("{base}_other").as_str())).then(|| (base.to_string(), category))
    };
    let mut plurals: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (k, t) in &pairs {
        match plural_base(k) {
            Some((base, category)) => {
                if !plurals.contains_key(&base) {
                    c.entry(&base);
                }
                let text = t.replace("{{count}}", "#");
                plurals.entry(base).or_default().push((category, from_double_braces(&text)));
            }
            None => c.text(k, lang, from_double_braces(t)),
        }
    }
    for (base, forms) in plurals {
        c.entry(&base).plural = true;
        c.text(&base, lang, format_plural("count", &forms));
    }
    Ok(c.finish())
}

fn arb(content: &str, need_lang: impl Fn(Option<String>) -> Result<String>) -> Result<Vec<ImportedKey>> {
    let v: Map<String, Value> = serde_json::from_str(content).map_err(|e| bad(Format::Arb, e))?;
    let lang = need_lang(v.get("@@locale").and_then(Value::as_str).map(String::from))?;
    let mut c = Collector::default();
    for (k, t) in &v {
        if k.starts_with('@') {
            continue;
        }
        let Some(t) = t.as_str() else { continue };
        c.text(k, &lang, t.to_string());
        let description = v.get(&format!("@{k}")).and_then(|m| m.get("description")).and_then(Value::as_str);
        c.entry(k).description = description.map(String::from);
    }
    Ok(c.finish())
}

// ---- gettext, properties, Apple ------------------------------------------

fn c_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn quoted(s: &str) -> Option<String> {
    let s = s.trim();
    (s.len() >= 2 && s.starts_with('"') && s.ends_with('"')).then(|| c_unescape(&s[1..s.len() - 1]))
}

fn po(content: &str, need_lang: impl Fn(Option<String>) -> Result<String>) -> Result<Vec<ImportedKey>> {
    #[derive(Default)]
    struct Unit {
        msgid: Option<String>,
        msgstr: Option<String>,
        comments: Vec<String>,
    }
    let mut units: Vec<Unit> = vec![];
    let mut cur = Unit::default();
    // Which field continuation lines extend.
    let mut field: Option<&str> = None;
    let flush = |cur: &mut Unit, units: &mut Vec<Unit>| {
        if cur.msgid.is_some() {
            units.push(std::mem::take(cur));
        }
    };
    for (n, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            flush(&mut cur, &mut units);
            field = None;
        } else if let Some(c) = line.strip_prefix("#.") {
            if cur.msgid.is_some() {
                flush(&mut cur, &mut units);
            }
            cur.comments.push(c.trim().to_string());
        } else if line.starts_with('#') {
            continue;
        } else if line.starts_with('"') {
            let text = quoted(line).ok_or_else(|| bad(Format::Po, format!("line {}", n + 1)))?;
            match field {
                Some("msgid") => cur.msgid.get_or_insert_default().push_str(&text),
                Some("msgstr") => cur.msgstr.get_or_insert_default().push_str(&text),
                _ => {}
            }
        } else {
            let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let value = quoted(rest);
            match word {
                "msgid" => {
                    if cur.msgid.is_some() {
                        flush(&mut cur, &mut units);
                    }
                    cur.msgid = value;
                    field = Some("msgid");
                }
                "msgstr" | "msgstr[0]" => {
                    cur.msgstr = value;
                    field = Some("msgstr");
                }
                _ => field = None, // msgctxt, msgid_plural, msgstr[n]
            }
        }
    }
    flush(&mut cur, &mut units);
    let header = units.iter().find(|u| u.msgid.as_deref() == Some("")).and_then(|u| u.msgstr.clone());
    let declared = header.and_then(|h| {
        h.lines().find_map(|l| l.strip_prefix("Language:").map(|v| v.trim().to_string())).filter(|v| !v.is_empty())
    });
    let lang = need_lang(declared)?;
    let mut c = Collector::default();
    for u in units {
        let (Some(id), Some(text)) = (u.msgid, u.msgstr) else { continue };
        if id.is_empty() || text.is_empty() {
            continue;
        }
        c.text(&id, &lang, text);
        if !u.comments.is_empty() {
            c.entry(&id).description = Some(u.comments.join("\n"));
        }
    }
    Ok(c.finish())
}

fn properties_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut pending_high: Option<u16> = None;
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('u') => {
                let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                if let Ok(unit) = u16::from_str_radix(&hex, 16) {
                    match (pending_high.take(), unit) {
                        (Some(high), 0xDC00..=0xDFFF) => {
                            out.extend(char::decode_utf16([high, unit]).filter_map(|r| r.ok()));
                        }
                        (_, 0xD800..=0xDBFF) => pending_high = Some(unit),
                        (_, u) => out.extend(char::from_u32(u as u32)),
                    }
                }
            }
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn properties(content: &str, lang: &str) -> Result<Vec<ImportedKey>> {
    let mut c = Collector::default();
    let mut logical = String::new();
    let mut comment: Vec<String> = vec![];
    for raw in content.lines() {
        let line = raw.trim_start();
        if logical.is_empty() && (line.is_empty() || line.starts_with('#') || line.starts_with('!')) {
            if let Some(cm) = line.strip_prefix('#').or_else(|| line.strip_prefix('!')) {
                comment.push(cm.trim().to_string());
            } else {
                comment.clear();
            }
            continue;
        }
        // An odd number of trailing backslashes continues the line.
        let trailing = line.chars().rev().take_while(|c| *c == '\\').count();
        if trailing % 2 == 1 {
            logical.push_str(&line[..line.len() - 1]);
            continue;
        }
        logical.push_str(line);
        let entry = std::mem::take(&mut logical);
        let mut split = None;
        let mut escaped = false;
        for (i, ch) in entry.char_indices() {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '=' || ch == ':' || ch.is_whitespace() {
                split = Some(i);
                break;
            }
        }
        let (k, v) = match split {
            Some(i) => {
                let rest = entry[i..].trim_start();
                let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest).trim_start();
                (&entry[..i], rest)
            }
            None => (entry.as_str(), ""),
        };
        let key = properties_unescape(k);
        c.text(&key, lang, properties_unescape(v));
        if !comment.is_empty() {
            c.entry(&key).description = Some(comment.join("\n"));
        }
        comment.clear();
    }
    Ok(c.finish())
}

static APPLE_ENTRY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)"((?:[^"\\]|\\.)*)"\s*=\s*"((?:[^"\\]|\\.)*)"\s*;"#).unwrap());
static APPLE_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)/\*.*?\*/|(?m)^\s*//.*$").unwrap());

fn apple(content: &str, lang: &str) -> Result<Vec<ImportedKey>> {
    let clean = APPLE_COMMENT.replace_all(content, "");
    let mut c = Collector::default();
    for cap in APPLE_ENTRY.captures_iter(&clean) {
        c.text(&c_unescape(&cap[1]), lang, c_unescape(&cap[2]));
    }
    Ok(c.finish())
}

// ---- XML formats -----------------------------------------------------------

fn attr(e: &BytesStart, name: &str) -> Option<String> {
    let a = e.try_get_attribute(name).ok().flatten()?;
    a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok().map(|v| v.into_owned())
}

fn resolve_ref(r: &BytesRef) -> String {
    if let Ok(Some(c)) = r.resolve_char_ref() {
        return c.to_string();
    }
    let name = r.xml10_content();
    quick_xml::escape::resolve_predefined_entity(&name).map(String::from).unwrap_or_else(|| format!("&{name};"))
}

/// Text and references go to `sink` while `capture` is on.
fn xml_events(format: Format, content: &str, mut on: impl FnMut(XmlEvent) -> Result<()>) -> Result<()> {
    let mut reader = quick_xml::Reader::from_str(content);
    loop {
        match reader.read_event().map_err(|e| bad(format, e))? {
            Event::Start(e) => on(XmlEvent::Start(e.name().as_ref().to_string(), e.clone()))?,
            Event::Empty(e) => {
                on(XmlEvent::Start(e.name().as_ref().to_string(), e.clone()))?;
                on(XmlEvent::End(e.name().as_ref().to_string()))?;
            }
            Event::End(e) => on(XmlEvent::End(e.name().as_ref().to_string()))?,
            Event::Text(t) => on(XmlEvent::Text(t.xml10_content().into_owned()))?,
            Event::CData(t) => on(XmlEvent::Text(t.xml10_content().into_owned()))?,
            Event::GeneralRef(r) => on(XmlEvent::Text(resolve_ref(&r)))?,
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

enum XmlEvent<'a> {
    Start(String, BytesStart<'a>),
    End(String),
    Text(String),
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn android_unescape(s: &str) -> String {
    let s = s.trim();
    let s = if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') { &s[1..s.len() - 1] } else { s };
    c_unescape(s)
}

fn android(content: &str, lang: &str) -> Result<Vec<ImportedKey>> {
    let mut c = Collector::default();
    let mut current: Option<(String, Option<String>)> = None; // (name, plural quantity)
    let mut plural: Option<(String, Vec<(String, String)>)> = None;
    let mut text = String::new();
    let mut depth_in_value = 0usize;
    xml_events(Format::AndroidXml, content, |ev| {
        match ev {
            XmlEvent::Start(name, e) => match local(&name) {
                "string" if depth_in_value == 0 => {
                    current = attr(&e, "name").map(|n| (n, None));
                    text.clear();
                }
                "plurals" => plural = attr(&e, "name").map(|n| (n, vec![])),
                "item" if plural.is_some() => {
                    current = Some((String::new(), attr(&e, "quantity")));
                    text.clear();
                }
                _ if current.is_some() => depth_in_value += 1, // inline markup: keep the text only
                _ => {}
            },
            XmlEvent::Text(t) => {
                if current.is_some() {
                    text.push_str(&t);
                }
            }
            XmlEvent::End(name) => match local(&name) {
                "string" if depth_in_value == 0 => {
                    if let Some((n, _)) = current.take() {
                        c.text(&n, lang, android_unescape(&text));
                    }
                }
                "item" if depth_in_value == 0 => {
                    if let (Some((_, Some(q))), Some((_, forms))) = (current.take(), plural.as_mut()) {
                        forms.push((q, android_unescape(&text).replace("%d", "#")));
                    }
                }
                "plurals" => {
                    if let Some((n, forms)) = plural.take()
                        && forms.iter().any(|(q, _)| q == "other")
                    {
                        c.entry(&n).plural = true;
                        c.text(&n, lang, format_plural("count", &forms));
                    }
                }
                _ if depth_in_value > 0 => depth_in_value -= 1,
                _ => {}
            },
        }
        Ok(())
    })?;
    Ok(c.finish())
}

/// XLIFF 1.2 (`trans-unit`) and 2.x (`unit`/`segment`). Sources are imported
/// into the source language, targets into the target language.
fn xliff(content: &str, language: Option<&str>) -> Result<Vec<ImportedKey>> {
    let mut c = Collector::default();
    let (mut src_lang, mut tgt_lang) = (None::<String>, None::<String>);
    let mut unit: Option<String> = None;
    let mut field: Option<&'static str> = None;
    let (mut source, mut target, mut note) = (String::new(), None::<String>, None::<String>);
    xml_events(Format::Xliff, content, |ev| {
        match ev {
            XmlEvent::Start(name, e) => match local(&name) {
                "xliff" => {
                    src_lang = attr(&e, "srcLang").or(src_lang.take());
                    tgt_lang = attr(&e, "trgLang").or(tgt_lang.take());
                }
                "file" => {
                    src_lang = attr(&e, "source-language").or(src_lang.take());
                    tgt_lang = attr(&e, "target-language").or(tgt_lang.take());
                }
                "trans-unit" | "unit" => {
                    unit = attr(&e, "resname").or_else(|| attr(&e, "name")).or_else(|| attr(&e, "id"));
                    (source, target, note) = (String::new(), None, None);
                }
                "source" if unit.is_some() => field = Some("source"),
                "target" if unit.is_some() => {
                    field = Some("target");
                    target.get_or_insert_default();
                }
                "note" if unit.is_some() => {
                    field = Some("note");
                    note.get_or_insert_default();
                }
                _ => {}
            },
            XmlEvent::Text(t) => match field {
                Some("source") => source.push_str(&t),
                Some("target") => target.get_or_insert_default().push_str(&t),
                Some("note") => note.get_or_insert_default().push_str(&t),
                _ => {}
            },
            XmlEvent::End(name) => match local(&name) {
                "source" | "target" | "note" => field = None,
                "trans-unit" | "unit" => {
                    if let Some(name) = unit.take() {
                        let tgt = language.map(String::from).or(tgt_lang.clone());
                        if let Some(l) = &src_lang
                            && !source.is_empty()
                            && tgt.as_deref() != Some(l.as_str())
                        {
                            c.text(&name, &normalize_tag(l)?, std::mem::take(&mut source));
                        }
                        match (&tgt, target.take().filter(|t| !t.is_empty())) {
                            (Some(l), Some(t)) => c.text(&name, &normalize_tag(l)?, t),
                            (None, Some(_)) => {
                                return Err(CmsError::validation("XLIFF: the file has no target-language; choose one"));
                            }
                            _ => {}
                        }
                        if let Some(n) = note.take().filter(|n| !n.trim().is_empty()) {
                            c.entry(&name).description = Some(n.trim().to_string());
                        } else {
                            c.entry(&name);
                        }
                    }
                }
                _ => {}
            },
        }
        Ok(())
    })?;
    Ok(c.finish())
}

// ---- CSV -----------------------------------------------------------------

fn csv_rows(content: &str) -> Result<Vec<Vec<String>>> {
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = content.chars().peekable();
    let mut any = false;
    while let Some(ch) = chars.next() {
        any = true;
        if in_quotes {
            match ch {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => in_quotes = false,
                c => field.push(c),
            }
            continue;
        }
        match ch {
            '"' if field.is_empty() => in_quotes = true,
            ',' => row.push(std::mem::take(&mut field)),
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            c => field.push(c),
        }
    }
    if in_quotes {
        return Err(bad(Format::Csv, "unterminated quoted field"));
    }
    if any && (!field.is_empty() || !row.is_empty()) {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].is_empty()));
    Ok(rows)
}

fn csv(content: &str) -> Result<Vec<ImportedKey>> {
    let rows = csv_rows(content)?;
    let Some((header, body)) = rows.split_first() else { return Ok(vec![]) };
    let col = |name: &str| header.iter().position(|h| h.trim().eq_ignore_ascii_case(name));
    let key_col = col("key").ok_or_else(|| bad(Format::Csv, "the first row must name a `key` column"))?;
    let desc_col = col("description");
    let skip = ["key", "description", "namespace", "tags"];
    let langs: Vec<(usize, String)> = header
        .iter()
        .enumerate()
        .filter(|(_, h)| !skip.contains(&h.trim().to_ascii_lowercase().as_str()))
        .map(|(i, h)| Ok((i, normalize_tag(h)?)))
        .collect::<Result<_>>()?;
    let mut c = Collector::default();
    for row in body {
        let Some(name) = row.get(key_col).filter(|k| !k.is_empty()) else { continue };
        c.entry(name).description = desc_col.and_then(|i| row.get(i)).filter(|d| !d.is_empty()).cloned();
        for (i, lang) in &langs {
            if let Some(t) = row.get(*i).filter(|t| !t.is_empty()) {
                c.text(name, lang, t.clone());
            }
        }
    }
    Ok(c.finish())
}
