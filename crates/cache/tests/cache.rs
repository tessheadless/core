// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::json;
use tess_cache::{CacheConfig, CacheService, CacheStore, MemoryStore, Scope, ValkeyStore};
use tess_core::memory::MemoryStorage;
use tess_core::{Actor, Cms, ContentTypeDef, FieldDef, FieldType};

async fn eventually(what: &str, check: impl Fn() -> bool) {
    let start = Instant::now();
    while !check() {
        assert!(start.elapsed() < Duration::from_secs(5), "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Shared scenario run against every store.
async fn scenario(store: Arc<dyn CacheStore>, prefix: &str) {
    let cache = CacheService::new(store, CacheConfig { ttl: Duration::from_millis(400), prefix: prefix.into() });
    let announced = Arc::new(AtomicUsize::new(0));
    let counter = announced.clone();
    cache.on_schema_change(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    tokio::time::sleep(Duration::from_millis(200)).await; // let the subscription start

    let q: &[&[u8]] = &[b"{ posts { title } }", b"{}"];
    let miss = cache.lookup(Scope::Content, q).await;
    assert!(miss.value.is_none());
    let key = miss.key.expect("store available");
    cache.store(&key, b"cached").await;
    assert_eq!(cache.lookup(Scope::Content, q).await.value.as_deref(), Some(&b"cached"[..]));
    assert!(cache.lookup(Scope::Content, &[b"{ posts { title } }", b"{\"x\":1}"]).await.value.is_none());

    // Entries expire on their TTL.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(cache.lookup(Scope::Content, q).await.value.is_none(), "expired");

    // Scopes are invalidated separately.
    let t: &[&[u8]] = &[b"json", b"fr.json"];
    cache.store(&cache.lookup(Scope::Content, q).await.key.unwrap(), b"c").await;
    cache.store(&cache.lookup(Scope::Translations, t).await.key.unwrap(), b"t").await;
    cache.invalidate(Scope::Content).await;
    assert!(cache.lookup(Scope::Content, q).await.value.is_none());
    assert_eq!(cache.lookup(Scope::Translations, t).await.value.as_deref(), Some(&b"t"[..]));

    // The hook invalidates on writes and announces model changes.
    let cms = Cms::new(Arc::new(MemoryStorage::new()));
    cms.hooks().add(Arc::new(cache.hook()));
    let sys = Actor::system();
    let fill = || async { cache.store(&cache.lookup(Scope::Content, q).await.key.unwrap(), b"c").await };
    fill().await;
    let ct = cms
        .create_content_type(
            ContentTypeDef {
                workflow: false,
                api_id: "CacheTkPost".into(),
                plural_api_id: None,
                description: None,
                fields: vec![FieldDef { name: "title".into(), field_type: FieldType::text(), required: false, description: None }],
            },
            &sys,
        )
        .await
        .unwrap();
    assert!(cache.lookup(Scope::Content, q).await.value.is_none(), "model change");
    eventually("the schema announcement", || announced.load(Ordering::SeqCst) >= 1).await;

    fill().await;
    let data = json!({"title": "Hello"}).as_object().unwrap().clone();
    cms.create_entry(&ct.api_id, data, &sys).await.unwrap();
    assert!(cache.lookup(Scope::Content, q).await.value.is_none(), "entry write");
}

#[tokio::test]
async fn memory_store() {
    scenario(Arc::new(MemoryStore::default()), "tess:").await;
}

#[tokio::test]
async fn memory_store_is_bounded() {
    let store = MemoryStore::new(2);
    for k in ["a", "b", "c"] {
        store.set(k, b"v", Duration::from_secs(60)).await.unwrap();
    }
    assert!(store.get("c").await.unwrap().is_some());
    assert!(store.get("a").await.unwrap().is_none(), "full: cleared before inserting");
}

/// Runs against `TESS_TEST_VALKEY_URL` (e.g. `redis://localhost:6379`); skipped when unset.
#[tokio::test]
async fn valkey_store() {
    let Ok(url) = std::env::var("TESS_TEST_VALKEY_URL") else {
        eprintln!("TESS_TEST_VALKEY_URL not set, skipping");
        return;
    };
    let prefix = format!("tesstest:{}:", std::process::id());
    scenario(Arc::new(ValkeyStore::new(&url).unwrap()), &prefix).await;
}

#[tokio::test]
async fn unreachable_server_is_skipped() {
    for url in ["redis://127.0.0.1:1", "rediss://127.0.0.1:1"] {
        let cache = CacheService::new(Arc::new(ValkeyStore::new(url).unwrap()), CacheConfig::default());
        let start = Instant::now();
        let lookup = cache.lookup(Scope::Content, &[b"q"]).await;
        assert!(lookup.key.is_none() && lookup.value.is_none(), "{url}: do not cache");
        // Later requests skip it at once until the retry delay.
        let again = Instant::now();
        assert!(cache.lookup(Scope::Content, &[b"q"]).await.key.is_none());
        assert!(again.elapsed() < Duration::from_millis(50), "{url}: no new attempt");
        assert!(start.elapsed() < Duration::from_secs(5));
        cache.invalidate(Scope::Content).await; // logged, never fails
    }
}
