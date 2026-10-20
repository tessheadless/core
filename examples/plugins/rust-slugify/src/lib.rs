// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Example CMS plugin: fills a slug field from a source field and rejects
//! entries containing forbidden words.
//!
//! Plugin config (JSON, set in the admin "Plugins" page):
//! `{"sourceField": "title", "slugField": "slug", "forbiddenWords": ["spam"]}`
//!
//! Build: `cargo build --release --target wasm32-unknown-unknown`
//! Output: `target/wasm32-unknown-unknown/release/slugify_plugin.wasm`

use extism_pdk::*;
use serde_json::{Value, json};

#[plugin_fn]
pub fn manifest() -> FnResult<String> {
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Generates URL slugs and blocks forbidden words.",
    })
    .to_string())
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

#[plugin_fn]
pub fn before_write(input: String) -> FnResult<String> {
    let input: Value = serde_json::from_str(&input)?;
    let action = input["action"].as_str().unwrap_or_default();
    if action != "create" && action != "update" {
        return Ok("{}".into());
    }
    let config = &input["config"];
    let source = config["sourceField"].as_str().unwrap_or("title");
    let target = config["slugField"].as_str().unwrap_or("slug");
    let mut data = input["data"].as_object().cloned().unwrap_or_default();
    let text = data.get(source).and_then(Value::as_str).unwrap_or_default().to_string();

    let lower = text.to_lowercase();
    if let Some(words) = config["forbiddenWords"].as_array() {
        for w in words.iter().filter_map(Value::as_str) {
            if !w.is_empty() && lower.contains(&w.to_lowercase()) {
                return Ok(json!({ "error": format!("`{source}` contains the forbidden word `{w}`") }).to_string());
            }
        }
    }
    let has_slug = data.get(target).and_then(Value::as_str).is_some_and(|s| !s.is_empty());
    if !text.is_empty() && !has_slug {
        data.insert(target.to_string(), Value::String(slugify(&text)));
        return Ok(json!({ "data": data }).to_string());
    }
    Ok("{}".into())
}
