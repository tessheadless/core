// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Runs the example slugify plugin through the plugin manager. Needs the
//! module built first (see `examples/plugins/rust-slugify`); the path comes
//! from `TESS_TEST_PLUGIN_WASM`, and the test is skipped when unset.

use std::sync::Arc;

use bytes::Bytes;
use tess_core::memory::MemoryStorage;
use tess_core::{Cms, CmsError, ContentTypeDef, FieldDef, FieldType, Storage};
use tess_media::FsBlobStore;
use tess_plugins::{PluginLimits, PluginManager, PluginSettings};
use serde_json::json;

fn data(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    v.as_object().unwrap().clone()
}

#[tokio::test]
async fn slugify_plugin_lifecycle() {
    let Ok(path) = std::env::var("TESS_TEST_PLUGIN_WASM") else {
        eprintln!("TESS_TEST_PLUGIN_WASM not set, skipping");
        return;
    };
    let wasm = Bytes::from(std::fs::read(path).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let cms = Cms::new(storage.clone());
    let refs: tess_plugins::RefFn = Arc::new(|ct: &str, id: uuid::Uuid| format!("{ct}-{}", &id.to_string()[..4]));
    let plugins = PluginManager::new(storage, Arc::new(FsBlobStore::new(dir.path())), refs, PluginLimits::default());

    let f = |name: &str, required| FieldDef { name: name.into(), field_type: FieldType::text(), required, description: None };
    cms.create_content_type(ContentTypeDef { workflow: false,
        api_id: "Post".into(),
        plural_api_id: None,
        description: None,
        fields: vec![f("title", true), f("slug", false)],
    }, &tess_core::Actor::system())
    .await
    .unwrap();

    // Validation of uploads.
    assert!(matches!(plugins.install("Bad Name", wasm.clone()).await, Err(CmsError::Validation(_))));
    assert!(matches!(plugins.install("junk", Bytes::from_static(b"nope")).await, Err(CmsError::Validation(_))));

    let record = plugins.install("slugify", wasm.clone()).await.unwrap();
    assert_eq!(record.version.as_deref(), Some("1.0.0"));
    assert_eq!(record.exports, vec!["before_write"]);
    assert!(!record.enabled, "plugins are installed disabled");

    // Disabled: no effect.
    plugins.refresh(&cms).await.unwrap();
    let e = cms.create_entry("Post", data(json!({"title": "Été à Paris"})), &tess_core::Actor::system()).await.unwrap();
    assert!(!e.draft.contains_key("slug"));

    plugins
        .update(
            "slugify",
            PluginSettings {
                enabled: Some(true),
                config: Some(json!({"forbiddenWords": ["casino"]})),
                content_types: Some(vec!["Post".into()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    plugins.refresh(&cms).await.unwrap();
    assert_eq!(cms.hooks().names(), vec!["plugin:slugify"]);
    assert!(plugins.get("slugify").await.unwrap().loaded);

    let e = cms.create_entry("Post", data(json!({"title": "Été à Paris !"})), &tess_core::Actor::system()).await.unwrap();
    assert_eq!(e.draft["slug"], "ete-a-paris");
    let kept = cms.create_entry("Post", data(json!({"title": "X", "slug": "custom"})), &tess_core::Actor::system()).await.unwrap();
    assert_eq!(kept.draft["slug"], "custom");
    let rejected = cms.create_entry("Post", data(json!({"title": "Best CASINO deals"})), &tess_core::Actor::system()).await;
    assert!(matches!(&rejected, Err(CmsError::Validation(m)) if m.contains("casino")), "{rejected:?}");

    // Bad sandbox settings are refused.
    let bad = PluginSettings { allowed_hosts: Some(vec!["http://x".into()]), ..Default::default() };
    assert!(plugins.update("slugify", bad).await.is_err());
    let bad = PluginSettings { timeout_ms: Some(10_000_000), ..Default::default() };
    assert!(plugins.update("slugify", bad).await.is_err());

    // Disabling and uninstalling unregisters the hook.
    plugins.update("slugify", PluginSettings { enabled: Some(false), ..Default::default() }).await.unwrap();
    plugins.refresh(&cms).await.unwrap();
    assert!(cms.hooks().names().is_empty());
    plugins.uninstall("slugify").await.unwrap();
    assert!(plugins.list().await.unwrap().is_empty());
    assert!(std::fs::read_dir(dir.path().join("plugins/slugify")).unwrap().next().is_none());
}
