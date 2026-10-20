// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Admin-installable plugins, as sandboxed WebAssembly modules (Extism).
//!
//! A plugin is a `.wasm` file built with any Extism PDK (Rust, JS, Go,
//! Python, Zig, C#, ...). It can export these functions, all taking and
//! returning JSON:
//!
//! | export                | input                                               | output |
//! |-----------------------|-----------------------------------------------------|--------|
//! | `manifest` (optional) | none                                                | `{"version": "1.0.0", "description": "..."}` |
//! | `before_write`        | `{"action","contentType","data","config"}`          | `{}` (no change), `{"data": {...}}` (replace), or `{"error": "message"}` (reject) |
//! | `after_write`         | `{"action","contentType","entry","previous","config"}` | ignored |
//!
//! `action` is one of `create`, `update`, `delete`, `publish`, `unpublish`,
//! and for content types with review workflow `submit`, `approve`,
//! `requestChanges`. Both functions also receive `actor` (`{"name", "role"}`).
//! Entries are given as `{"ref","draft","published","createdAt","updatedAt","publishedAt"}`:
//! the stable `ref` identifies an entry, storage ids never reach plugins.
//!
//! Plugins run without filesystem or environment access, under a timeout and a
//! memory cap. Outgoing HTTP is denied unless hosts are allow-listed per plugin.

mod hook;
mod manager;

pub use hook::WasmHook;
pub use manager::{PluginLimits, PluginManager, PluginSettings, PluginStatus, RefFn};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Persisted state of an installed plugin (settings key `plugins/{name}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRecord {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    /// SHA-256 of the module, hex.
    pub sha256: String,
    pub size: u64,
    /// Blob store key of the module.
    pub module_key: String,
    /// Hook functions the module exports.
    pub exports: Vec<String>,
    pub enabled: bool,
    /// Free-form configuration passed to every call.
    pub config: Value,
    /// Content types the plugin applies to; empty means all.
    pub content_types: Vec<String>,
    /// Hosts the plugin may send HTTP requests to (`*.example.com` allowed).
    pub allowed_hosts: Vec<String>,
    pub timeout_ms: u64,
    /// Memory cap in 64 KiB WebAssembly pages.
    pub memory_max_pages: u32,
    /// When true, a crashing `before_write` is logged and ignored instead of
    /// rejecting the write.
    pub fail_open: bool,
    pub installed_at: String,
    pub updated_at: String,
}

/// Hook functions a plugin may export.
pub const HOOK_EXPORTS: &[&str] = &["before_write", "after_write"];
