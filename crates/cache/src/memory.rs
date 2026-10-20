// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tess_core::Result;
use tokio::sync::broadcast;

use crate::{CacheStore, MessageFn};

/// In-process store, for a single instance (and tests). Holds at most
/// `max_entries` values; expired ones are dropped first, then everything.
pub struct MemoryStore {
    values: Mutex<HashMap<String, (Vec<u8>, Instant)>>,
    counters: Mutex<HashMap<String, u64>>,
    max_entries: usize,
    messages: broadcast::Sender<(String, String)>,
}

impl MemoryStore {
    pub fn new(max_entries: usize) -> Self {
        MemoryStore {
            values: Mutex::default(),
            counters: Mutex::default(),
            max_entries: max_entries.max(1),
            messages: broadcast::channel(64).0,
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        MemoryStore::new(10_000)
    }
}

#[async_trait]
impl CacheStore for MemoryStore {
    fn name(&self) -> &'static str {
        "memory"
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let mut values = self.values.lock().expect("cache lock");
        match values.get(key) {
            Some((v, expires)) if *expires > Instant::now() => Ok(Some(v.clone())),
            Some(_) => {
                values.remove(key);
                Ok(None)
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()> {
        let mut values = self.values.lock().expect("cache lock");
        if values.len() >= self.max_entries {
            let now = Instant::now();
            values.retain(|_, (_, expires)| *expires > now);
            if values.len() >= self.max_entries {
                values.clear();
            }
        }
        values.insert(key.to_string(), (value.to_vec(), Instant::now() + ttl));
        Ok(())
    }

    async fn counter(&self, key: &str) -> Result<u64> {
        Ok(self.counters.lock().expect("cache lock").get(key).copied().unwrap_or(0))
    }

    async fn increment(&self, key: &str) -> Result<u64> {
        let mut counters = self.counters.lock().expect("cache lock");
        let n = counters.entry(key.to_string()).or_insert(0);
        *n += 1;
        Ok(*n)
    }

    async fn publish(&self, channel: &str, message: &str) -> Result<()> {
        // No receiver is fine.
        let _ = self.messages.send((channel.to_string(), message.to_string()));
        Ok(())
    }

    fn subscribe(&self, channel: &str, on_message: MessageFn) {
        let mut rx = self.messages.subscribe();
        let channel = channel.to_string();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok((c, m)) if c == channel => on_message(&m),
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => on_message(""),
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
    }
}
