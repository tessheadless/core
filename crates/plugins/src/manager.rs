// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use bytes::Bytes;
use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tess_core::model::{format_ts, now};
use tess_core::{Cms, CmsError, Hook, Result, Storage};
use tess_media::BlobStore;
use uuid::Uuid;

use crate::hook::{WasmHook, call, instantiate};
use crate::{HOOK_EXPORTS, PluginRecord};

const SETTINGS_PREFIX: &str = "plugins/";
const HOOK_PREFIX: &str = "plugin:";
static NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[a-z][a-z0-9-]{1,47}$").unwrap());
static HOST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\*\.)?[A-Za-z0-9.-]{1,253}(:\d{1,5})?$").unwrap());

/// Computes the stable, non-reversible `ref` of an entry (content type, id).
pub type RefFn = Arc<dyn Fn(&str, Uuid) -> String + Send + Sync>;

/// Operator-level bounds; per-plugin settings are clamped to them.
#[derive(Debug, Clone)]
pub struct PluginLimits {
    pub max_module_bytes: usize,
    pub max_timeout_ms: u64,
    pub max_memory_pages: u32,
    /// Whether plugins may be allowed to make HTTP requests at all.
    pub allow_http: bool,
}

impl Default for PluginLimits {
    fn default() -> Self {
        PluginLimits {
            max_module_bytes: 20 * 1024 * 1024,
            max_timeout_ms: 10_000,
            max_memory_pages: 1024,
            allow_http: true,
        }
    }
}

/// Settings an admin can change; `None` keeps the current value.
#[derive(Debug, Clone, Default)]
pub struct PluginSettings {
    pub enabled: Option<bool>,
    pub config: Option<Value>,
    pub content_types: Option<Vec<String>>,
    pub allowed_hosts: Option<Vec<String>>,
    pub timeout_ms: Option<u64>,
    pub memory_max_pages: Option<u32>,
    pub fail_open: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct PluginStatus {
    pub record: PluginRecord,
    /// Whether the plugin is currently running on this instance.
    pub loaded: bool,
    /// Last runtime error seen on this instance.
    pub last_error: Option<String>,
}

struct Loaded {
    fingerprint: String,
    hooks: HashMap<String, Arc<WasmHook>>,
}

pub struct PluginManager {
    storage: Arc<dyn Storage>,
    blobs: Arc<dyn BlobStore>,
    refs: RefFn,
    limits: PluginLimits,
    loaded: tokio::sync::Mutex<Loaded>,
    errors: Arc<Mutex<HashMap<String, String>>>,
}

fn key(name: &str) -> String {
    format!("{SETTINGS_PREFIX}{name}")
}

impl PluginManager {
    pub fn new(
        storage: Arc<dyn Storage>,
        blobs: Arc<dyn BlobStore>,
        refs: RefFn,
        limits: PluginLimits,
    ) -> Self {
        PluginManager {
            storage,
            blobs,
            refs,
            limits,
            loaded: tokio::sync::Mutex::new(Loaded {
                fingerprint: String::new(),
                hooks: HashMap::new(),
            }),
            errors: Arc::default(),
        }
    }

    pub fn limits(&self) -> &PluginLimits {
        &self.limits
    }

    async fn records(&self) -> Result<Vec<PluginRecord>> {
        self.storage
            .list_settings(SETTINGS_PREFIX)
            .await?
            .into_iter()
            .map(|(_, v)| serde_json::from_value(v).map_err(CmsError::storage))
            .collect()
    }

    async fn record(&self, name: &str) -> Result<PluginRecord> {
        let v = self
            .storage
            .get_setting(&key(name))
            .await?
            .ok_or_else(|| CmsError::NotFound(format!("plugin `{name}`")))?;
        serde_json::from_value(v).map_err(CmsError::storage)
    }

    async fn save(&self, record: &PluginRecord) -> Result<()> {
        let v = serde_json::to_value(record).map_err(CmsError::storage)?;
        self.storage.put_setting(&key(&record.name), &v).await
    }

    pub async fn list(&self) -> Result<Vec<PluginStatus>> {
        let loaded = self.loaded.lock().await;
        let errors = self.errors.lock().map(|e| e.clone()).unwrap_or_default();
        Ok(self
            .records()
            .await?
            .into_iter()
            .map(|record| PluginStatus {
                loaded: loaded
                    .hooks
                    .get(&record.name)
                    .is_some_and(|h| h.record == record),
                last_error: errors.get(&record.name).cloned(),
                record,
            })
            .collect())
    }

    pub async fn get(&self, name: &str) -> Result<PluginStatus> {
        self.list()
            .await?
            .into_iter()
            .find(|s| s.record.name == name)
            .ok_or_else(|| CmsError::NotFound(format!("plugin `{name}`")))
    }

    /// Installs a new plugin (disabled) or replaces the module of an existing
    /// one (settings are kept). The module is loaded once to validate it.
    pub async fn install(&self, name: &str, wasm: Bytes) -> Result<PluginRecord> {
        if !NAME.is_match(name) {
            return Err(CmsError::validation(
                "plugin name must be 2-48 chars of lowercase letters, digits and `-`, starting with a letter",
            ));
        }
        if wasm.len() > self.limits.max_module_bytes {
            return Err(CmsError::validation(format!(
                "module too large ({} bytes, max {})",
                wasm.len(),
                self.limits.max_module_bytes
            )));
        }
        if !wasm.starts_with(b"\0asm") {
            return Err(CmsError::validation("not a WebAssembly module"));
        }
        let sha256: String = Sha256::digest(&wasm)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let existing = self.record(name).await.ok();
        let ts = format_ts(now());
        let mut record = existing.clone().unwrap_or(PluginRecord {
            name: name.to_string(),
            version: None,
            description: None,
            sha256: String::new(),
            size: 0,
            module_key: String::new(),
            exports: vec![],
            enabled: false,
            config: Value::Object(Default::default()),
            content_types: vec![],
            allowed_hosts: vec![],
            timeout_ms: 1000.min(self.limits.max_timeout_ms),
            memory_max_pages: 256.min(self.limits.max_memory_pages),
            fail_open: false,
            installed_at: ts.clone(),
            updated_at: ts.clone(),
        });
        record.sha256 = sha256.clone();
        record.size = wasm.len() as u64;
        record.module_key = format!("plugins/{name}/{sha256}.wasm");
        record.updated_at = ts;

        // Inspect the module in a sandbox without network access.
        let inspect = PluginRecord {
            allowed_hosts: vec![],
            ..record.clone()
        };
        let instance = {
            let wasm = wasm.clone();
            tokio::task::spawn_blocking(move || instantiate(&wasm, &inspect))
                .await
                .map_err(CmsError::storage)??
        };
        record.exports = HOOK_EXPORTS
            .iter()
            .filter(|f| instance.function_exists(f))
            .map(|f| f.to_string())
            .collect();
        if record.exports.is_empty() {
            return Err(CmsError::validation(format!(
                "the module exports none of {HOOK_EXPORTS:?}"
            )));
        }
        if instance.function_exists("manifest") {
            let out = call(Arc::new(Mutex::new(instance)), "manifest", vec![])
                .await
                .map_err(|e| CmsError::Plugin(format!("`{name}` manifest failed: {e}")))?;
            let manifest: Value = serde_json::from_slice(&out)
                .map_err(|e| CmsError::Plugin(format!("`{name}` manifest is not JSON: {e}")))?;
            record.version = manifest
                .get("version")
                .and_then(Value::as_str)
                .map(String::from);
            record.description = manifest
                .get("description")
                .and_then(Value::as_str)
                .map(String::from);
        }

        self.blobs
            .put(&record.module_key, wasm, "application/wasm")
            .await?;
        self.save(&record).await?;
        if let Some(old) = existing
            && old.module_key != record.module_key
        {
            let _ = self.blobs.delete(&old.module_key).await;
        }
        if let Ok(mut errors) = self.errors.lock() {
            errors.remove(name);
        }
        Ok(record)
    }

    pub async fn update(&self, name: &str, settings: PluginSettings) -> Result<PluginRecord> {
        let mut record = self.record(name).await?;
        if let Some(v) = settings.enabled {
            record.enabled = v;
        }
        if let Some(v) = settings.config {
            record.config = v;
        }
        if let Some(v) = settings.content_types {
            record.content_types = v;
        }
        if let Some(hosts) = settings.allowed_hosts {
            if !hosts.is_empty() && !self.limits.allow_http {
                return Err(CmsError::validation(
                    "outgoing HTTP from plugins is disabled on this server",
                ));
            }
            if let Some(bad) = hosts.iter().find(|h| !HOST.is_match(h)) {
                return Err(CmsError::validation(format!("invalid host `{bad}`")));
            }
            record.allowed_hosts = hosts;
        }
        if let Some(v) = settings.timeout_ms {
            if v == 0 || v > self.limits.max_timeout_ms {
                return Err(CmsError::validation(format!(
                    "timeout must be 1..={} ms",
                    self.limits.max_timeout_ms
                )));
            }
            record.timeout_ms = v;
        }
        if let Some(v) = settings.memory_max_pages {
            if v == 0 || v > self.limits.max_memory_pages {
                return Err(CmsError::validation(format!(
                    "memory must be 1..={} pages of 64 KiB",
                    self.limits.max_memory_pages
                )));
            }
            record.memory_max_pages = v;
        }
        if let Some(v) = settings.fail_open {
            record.fail_open = v;
        }
        record.updated_at = format_ts(now());
        self.save(&record).await?;
        if let Ok(mut errors) = self.errors.lock() {
            errors.remove(name);
        }
        Ok(record)
    }

    pub async fn uninstall(&self, name: &str) -> Result<()> {
        let record = self.record(name).await?;
        self.storage.delete_setting(&key(name)).await?;
        let _ = self.blobs.delete(&record.module_key).await;
        if let Ok(mut errors) = self.errors.lock() {
            errors.remove(name);
        }
        Ok(())
    }

    /// Loads enabled plugins and registers them as hooks on `cms`. Cheap when
    /// nothing changed; call it on startup and periodically.
    pub async fn refresh(&self, cms: &Cms) -> Result<()> {
        let mut enabled: Vec<PluginRecord> = self
            .records()
            .await?
            .into_iter()
            .filter(|r| r.enabled)
            .collect();
        enabled.sort_by(|a, b| a.name.cmp(&b.name));
        let fingerprint = serde_json::to_string(&enabled).map_err(CmsError::storage)?;
        let mut loaded = self.loaded.lock().await;
        if loaded.fingerprint == fingerprint {
            return Ok(());
        }
        let mut hooks = HashMap::new();
        for record in enabled {
            // Reuse instances whose module and sandbox settings did not change.
            if let Some(h) = loaded.hooks.get(&record.name)
                && h.record == record
            {
                hooks.insert(record.name.clone(), h.clone());
                continue;
            }
            match self.load(&record).await {
                Ok(hook) => {
                    hooks.insert(record.name.clone(), Arc::new(hook));
                }
                Err(e) => {
                    tracing::error!(plugin = %record.name, error = %e, "cannot load plugin");
                    if let Ok(mut errors) = self.errors.lock() {
                        errors.insert(record.name.clone(), e.to_string());
                    }
                }
            }
        }
        let mut ordered: Vec<_> = hooks.values().cloned().collect();
        ordered.sort_by(|a, b| a.record.name.cmp(&b.record.name));
        cms.hooks().replace_prefixed(
            HOOK_PREFIX,
            ordered.into_iter().map(|h| h as Arc<dyn Hook>).collect(),
        );
        tracing::info!(plugins = ?hooks.keys().collect::<Vec<_>>(), "plugins loaded");
        *loaded = Loaded { fingerprint, hooks };
        Ok(())
    }

    async fn load(&self, record: &PluginRecord) -> Result<WasmHook> {
        let wasm =
            self.blobs.get(&record.module_key).await?.ok_or_else(|| {
                CmsError::Plugin(format!("module of `{}` is missing", record.name))
            })?;
        let digest: String = Sha256::digest(&wasm)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if digest != record.sha256 {
            return Err(CmsError::Plugin(format!(
                "module of `{}` does not match its checksum",
                record.name
            )));
        }
        let sandbox = PluginRecord {
            allowed_hosts: if self.limits.allow_http {
                record.allowed_hosts.clone()
            } else {
                vec![]
            },
            ..record.clone()
        };
        let instance = tokio::task::spawn_blocking(move || instantiate(&wasm, &sandbox))
            .await
            .map_err(CmsError::storage)??;
        Ok(WasmHook::new(
            record.clone(),
            instance,
            self.refs.clone(),
            self.errors.clone(),
        ))
    }
}
