// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use tess_core::model::format_ts;
use tess_core::{Actor, CmsError, ContentType, Entry, EntryAction, EntryEvent, Hook, Result};

use crate::PluginRecord;
use crate::manager::RefFn;

/// Instantiates a module with the sandbox settings of `record`.
pub(crate) fn instantiate(wasm: &[u8], record: &PluginRecord) -> Result<extism::Plugin> {
    let manifest = extism::Manifest::new([extism::Wasm::data(wasm.to_vec())])
        .with_timeout(Duration::from_millis(record.timeout_ms))
        .with_memory_max(record.memory_max_pages)
        .with_allowed_hosts(record.allowed_hosts.clone().into_iter());
    extism::PluginBuilder::new(manifest)
        .with_wasi(true)
        .with_cache_disabled()
        .build()
        .map_err(|e| CmsError::Plugin(format!("`{}` cannot be loaded: {e:#}", record.name)))
}

/// Runs a plugin function on the blocking pool (plugins execute synchronously).
pub(crate) async fn call(
    instance: Arc<Mutex<extism::Plugin>>,
    function: &'static str,
    input: Vec<u8>,
) -> std::result::Result<Vec<u8>, String> {
    tokio::task::spawn_blocking(move || {
        let mut plugin = instance
            .lock()
            .map_err(|_| "plugin instance poisoned".to_string())?;
        let out = plugin
            .call::<&[u8], &[u8]>(function, &input)
            .map(|o| o.to_vec())
            .map_err(|e| format!("{e:#}"));
        // A trap or timeout can leave the instance in a bad state.
        if out.is_err() {
            let _ = plugin.reset();
        }
        out
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A loaded plugin, registered on the CMS as hook `plugin:{name}`.
pub struct WasmHook {
    hook_name: String,
    pub(crate) record: PluginRecord,
    instance: Arc<Mutex<extism::Plugin>>,
    refs: RefFn,
    errors: Arc<Mutex<HashMap<String, String>>>,
}

fn action_name(action: EntryAction) -> &'static str {
    match action {
        EntryAction::Create => "create",
        EntryAction::Update => "update",
        EntryAction::Delete => "delete",
        EntryAction::Publish => "publish",
        EntryAction::Unpublish => "unpublish",
        EntryAction::Submit => "submit",
        EntryAction::Approve => "approve",
        EntryAction::RequestChanges => "requestChanges",
    }
}

impl WasmHook {
    pub(crate) fn new(
        record: PluginRecord,
        instance: extism::Plugin,
        refs: RefFn,
        errors: Arc<Mutex<HashMap<String, String>>>,
    ) -> Self {
        WasmHook {
            hook_name: format!("plugin:{}", record.name),
            record,
            instance: Arc::new(Mutex::new(instance)),
            refs,
            errors,
        }
    }

    fn applies_to(&self, content_type: &str) -> bool {
        self.record.content_types.is_empty()
            || self.record.content_types.iter().any(|c| c == content_type)
    }

    fn exports(&self, f: &str) -> bool {
        self.record.exports.iter().any(|e| e == f)
    }

    fn record_error(&self, msg: &str) {
        tracing::warn!(plugin = %self.record.name, error = msg, "plugin failure");
        if let Ok(mut errors) = self.errors.lock() {
            errors.insert(self.record.name.clone(), msg.to_string());
        }
    }

    fn entry_json(&self, entry: &Entry) -> Value {
        json!({
            "ref": (self.refs)(&entry.content_type, entry.id),
            "draft": entry.draft,
            "published": entry.published,
            "createdAt": format_ts(entry.created_at),
            "updatedAt": format_ts(entry.updated_at),
            "publishedAt": entry.published_at.map(format_ts),
            "workflowStatus": entry.status.as_str(),
        })
    }
}

#[async_trait]
impl Hook for WasmHook {
    fn name(&self) -> &str {
        &self.hook_name
    }

    async fn before_write(
        &self,
        action: EntryAction,
        content_type: &ContentType,
        data: &mut Map<String, Value>,
        actor: &Actor,
    ) -> Result<()> {
        if !self.exports("before_write") || !self.applies_to(&content_type.api_id) {
            return Ok(());
        }
        let input = json!({
            "action": action_name(action),
            "contentType": content_type.api_id,
            "data": data,
            "actor": { "name": actor.name, "role": actor.role.as_str() },
            "config": self.record.config,
        });
        let name = &self.record.name;
        let output = match call(
            self.instance.clone(),
            "before_write",
            input.to_string().into_bytes(),
        )
        .await
        {
            Ok(out) => out,
            Err(e) => {
                self.record_error(&e);
                return if self.record.fail_open {
                    Ok(())
                } else {
                    Err(CmsError::Plugin(format!("`{name}` failed: {e}")))
                };
            }
        };
        if output.iter().all(u8::is_ascii_whitespace) {
            return Ok(());
        }
        let parsed: Value = match serde_json::from_slice(&output) {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("`{name}` returned invalid JSON: {e}");
                self.record_error(&msg);
                return if self.record.fail_open {
                    Ok(())
                } else {
                    Err(CmsError::Plugin(msg))
                };
            }
        };
        if let Some(err) = parsed.get("error").and_then(Value::as_str) {
            return Err(CmsError::Validation(err.to_string()));
        }
        if let Some(Value::Object(new_data)) = parsed.get("data") {
            *data = new_data.clone();
        }
        Ok(())
    }

    async fn after_write(&self, event: &EntryEvent) {
        if !self.exports("after_write") || !self.applies_to(&event.content_type) {
            return;
        }
        let input = json!({
            "action": action_name(event.action),
            "contentType": event.content_type,
            "entry": event.entry.as_ref().map(|e| self.entry_json(e)),
            "previous": event.previous.as_ref().map(|e| self.entry_json(e)),
            "actor": { "name": event.actor.name },
            "config": self.record.config,
        });
        if let Err(e) = call(
            self.instance.clone(),
            "after_write",
            input.to_string().into_bytes(),
        )
        .await
        {
            self.record_error(&e);
        }
    }
}
