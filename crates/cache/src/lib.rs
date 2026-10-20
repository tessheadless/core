// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Response cache. Values live in a [`CacheStore`]: [`ValkeyStore`] (Valkey,
//! or any Redis-compatible server) shared by every instance, or
//! [`MemoryStore`] for a single instance.
//!
//! Invalidation uses generations: each [`Scope`] has a counter in the store,
//! part of every key. A change increments it ([`CacheHook`] does on every
//! write), so all older entries stop matching at once and expire on their
//! TTL; nothing has to be listed or deleted. The hook also tells the other
//! instances, through the store's pub/sub, to rebuild their GraphQL schema
//! when the content model changes.
//!
//! The cache never fails a request: store errors are logged and the request
//! is served without it.

mod memory;
mod valkey;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tess_core::{ContentType, EntryEvent, Hook, MODEL_EVENTS, Result, SystemEvent};

pub use memory::MemoryStore;
pub use valkey::ValkeyStore;

/// Called for each message of a subscribed channel.
pub type MessageFn = Arc<dyn Fn(&str) + Send + Sync>;

#[async_trait]
pub trait CacheStore: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    async fn set(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()>;
    /// Current value of a counter (0 when missing).
    async fn counter(&self, key: &str) -> Result<u64>;
    async fn increment(&self, key: &str) -> Result<u64>;
    async fn publish(&self, channel: &str, message: &str) -> Result<()>;
    /// Calls `on_message` for every message published on `channel` (by any
    /// instance) from now on, reconnecting as needed. Also called with `""`
    /// after a reconnection, since messages may have been missed.
    fn subscribe(&self, channel: &str, on_message: MessageFn);
}

/// What a cached value depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Content, content models and components (GraphQL responses).
    Content,
    /// Translations (served files).
    Translations,
}

impl Scope {
    fn as_str(self) -> &'static str {
        match self {
            Scope::Content => "content",
            Scope::Translations => "i18n",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Lifetime of cached values.
    pub ttl: Duration,
    /// Prefix of every key and channel, to share a server between installs.
    pub prefix: String,
}

impl Default for CacheConfig {
    fn default() -> Self {
        CacheConfig { ttl: Duration::from_secs(60), prefix: "tess:".into() }
    }
}

/// A cache lookup: the value when cached, and the key to store it under
/// otherwise (`None` when the store is unavailable: do not cache).
#[derive(Debug, Default)]
pub struct Lookup {
    pub value: Option<Vec<u8>>,
    pub key: Option<String>,
}

#[derive(Clone)]
pub struct CacheService {
    store: Arc<dyn CacheStore>,
    config: CacheConfig,
}

impl CacheService {
    pub fn new(store: Arc<dyn CacheStore>, config: CacheConfig) -> Self {
        CacheService { store, config }
    }

    pub fn name(&self) -> &'static str {
        self.store.name()
    }

    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    fn generation_key(&self, scope: Scope) -> String {
        format!("{}gen:{}", self.config.prefix, scope.as_str())
    }

    /// Looks up the value identified by `parts` (hashed) in `scope`.
    pub async fn lookup(&self, scope: Scope, parts: &[&[u8]]) -> Lookup {
        let generation = match self.store.counter(&self.generation_key(scope)).await {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!(error = %e, "cache unavailable");
                return Lookup::default();
            }
        };
        let mut h = Sha256::new();
        for p in parts {
            h.update((p.len() as u64).to_le_bytes());
            h.update(p);
        }
        let key = format!("{}{}:{generation}:{}", self.config.prefix, scope.as_str(), hex::encode(h.finalize()));
        match self.store.get(&key).await {
            Ok(value) => Lookup { value, key: Some(key) },
            Err(e) => {
                tracing::warn!(error = %e, "cache read failed");
                Lookup::default()
            }
        }
    }

    /// Stores a value under a key from [`CacheService::lookup`].
    pub async fn store(&self, key: &str, value: &[u8]) {
        if let Err(e) = self.store.set(key, value, self.config.ttl).await {
            tracing::warn!(error = %e, "cache write failed");
        }
    }

    /// Invalidates every value of `scope`, on every instance.
    pub async fn invalidate(&self, scope: Scope) {
        if let Err(e) = self.store.increment(&self.generation_key(scope)).await {
            tracing::warn!(error = %e, scope = scope.as_str(), "cache invalidation failed");
        }
    }

    fn schema_channel(&self) -> String {
        format!("{}schema", self.config.prefix)
    }

    /// Tells every instance (this one included) that the content model changed.
    pub async fn announce_schema_change(&self) {
        if let Err(e) = self.store.publish(&self.schema_channel(), "changed").await {
            tracing::warn!(error = %e, "cannot announce the schema change");
        }
    }

    /// Runs `f` whenever an instance announces a schema change.
    pub fn on_schema_change(&self, f: impl Fn() + Send + Sync + 'static) {
        self.store.subscribe(&self.schema_channel(), Arc::new(move |_| f()));
    }

    /// The hook invalidating the cache on every change.
    pub fn hook(&self) -> CacheHook {
        CacheHook(self.clone())
    }
}

pub struct CacheHook(CacheService);

#[async_trait]
impl Hook for CacheHook {
    fn name(&self) -> &str {
        "builtin:cache"
    }

    async fn after_write(&self, _event: &EntryEvent) {
        self.0.invalidate(Scope::Content).await;
    }

    async fn after_content_type_change(&self, _content_type: &ContentType, _deleted: bool) {
        self.0.invalidate(Scope::Content).await;
        self.0.announce_schema_change().await;
    }

    async fn on_event(&self, event: &SystemEvent) {
        if event.event.starts_with("i18n.") {
            self.0.invalidate(Scope::Translations).await;
            return;
        }
        self.0.invalidate(Scope::Content).await;
        if MODEL_EVENTS.contains(&event.event.as_str()) {
            self.0.announce_schema_change().await;
        }
    }
}
