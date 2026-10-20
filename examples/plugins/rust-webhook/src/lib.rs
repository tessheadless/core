// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

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
