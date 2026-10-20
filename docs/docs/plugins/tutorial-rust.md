---
sidebar_position: 4
title: "Tutorial: Rust"
---

# Writing a plugin in Rust

We'll build `slugify`, which fills a `slug` field from `title` and rejects
forbidden words. It's the `examples/plugins/rust-slugify` example, which the
project's test suite builds and tests.

## 1. Set up

```bash
rustup target add wasm32-unknown-unknown
cargo new --lib slugify-plugin && cd slugify-plugin
```

`Cargo.toml`: a `cdylib` with the Extism PDK. The empty `[workspace]` table
keeps it independent of any surrounding workspace.

```toml
[package]
name = "slugify-plugin"
version = "1.0.0"
edition = "2024"
publish = false

# Standalone: not part of the CMS workspace.
[workspace]

[lib]
crate-type = ["cdylib"]

[dependencies]
extism-pdk = "1"
serde_json = "1"

[profile.release]
opt-level = "s"
lto = true
strip = true
```

## 2. Implement the hooks

`src/lib.rs`:

```rust
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
```

- `#[plugin_fn]` exports a function. The input is the JSON described in
  [the contract](contract.md), and the returned string is the output.
- Returning `{"data": …}` replaces the entry data; `{"error": …}` rejects
  the change.
- Only `create` and `update` are handled; other actions return `{}`.

## 3. Build

```bash
cargo build --release --target wasm32-unknown-unknown
ls target/wasm32-unknown-unknown/release/slugify_plugin.wasm
```

## 4. Install, configure, enable

In **Admin → Plugins**: upload the `.wasm` as `slugify`, select the content
types that have `title` and `slug` fields, set the config, and enable it.
Or use the API with an Admin token (see [Managing plugins](managing.md)):

```bash
curl -s -X POST https://cms.example.com/api/plugins/slugify \
  -H "authorization: Bearer $TOKEN" \
  --data-binary @target/wasm32-unknown-unknown/release/slugify_plugin.wasm
```

```graphql
mutation {
  updatePlugin(name: "slugify", input: {
    enabled: true
    contentTypes: ["Post"]
    config: { sourceField: "title", slugField: "slug", forbiddenWords: ["casino"] }
  }) { enabled loaded lastError }
}
```

## 5. Try it

```graphql
mutation { createPost(data: { title: "Été à Paris !" }) { slug } }
# → { "slug": "ete-a-paris" }

mutation { createPost(data: { title: "Best casino deals" }) { slug } }
# → error BAD_USER_INPUT: `title` contains the forbidden word `casino`
```

## Going further: `after_write` and HTTP

`examples/plugins/rust-webhook` notifies a URL after changes. It only sends
identifiers and metadata, never the content:

```rust
//! Example CMS plugin: POSTs a JSON notification to a webhook after every
//! change (e.g. to trigger a static site rebuild on publish).
//!
//! Config: `{"url": "https://hooks.example.com/cms", "actions": ["publish", "unpublish"]}`
//! The webhook host MUST be added to the plugin's "allowed hosts" in the
//! admin, otherwise the sandbox refuses the request.
//!
//! Build: `cargo build --release --target wasm32-unknown-unknown`

use extism_pdk::*;
use serde_json::{Value, json};

#[plugin_fn]
pub fn manifest() -> FnResult<String> {
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Sends a webhook when entries change.",
    })
    .to_string())
}

#[plugin_fn]
pub fn after_write(input: String) -> FnResult<()> {
    let input: Value = serde_json::from_str(&input)?;
    let config = &input["config"];
    let Some(url) = config["url"].as_str() else {
        return Ok(()); // not configured yet
    };
    let action = input["action"].as_str().unwrap_or_default();
    if let Some(actions) = config["actions"].as_array()
        && !actions.iter().any(|a| a.as_str() == Some(action))
    {
        return Ok(());
    }
    // Only send identifiers and metadata, not the full content.
    let entry = if input["entry"].is_null() { &input["previous"] } else { &input["entry"] };
    let body = json!({
        "event": format!("entry.{action}"),
        "contentType": input["contentType"],
        "ref": entry["ref"],
        "updatedAt": entry["updatedAt"],
        "publishedAt": entry["publishedAt"],
    });
    let req = HttpRequest::new(url).with_method("POST").with_header("Content-Type", "application/json");
    let res = http::request::<String>(&req, Some(body.to_string()))?;
    if res.status_code() >= 400 {
        return Err(Error::msg(format!("webhook answered HTTP {}", res.status_code())).into());
    }
    Ok(())
}
```

Configure it with `{"url": "https://hooks.example.com/tess", "actions": ["publish"]}`
and add `hooks.example.com` to its **allowed hosts**. Otherwise the sandbox
refuses the request, and the error shows up as the plugin's `lastError`.
