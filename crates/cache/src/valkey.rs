// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::StreamExt;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use tess_core::{CmsError, Result};
use tokio::sync::Mutex;

use crate::{CacheStore, MessageFn};

/// After a failed connection, requests skip the cache this long before the
/// next attempt (so a down server costs nothing per request).
const RETRY_AFTER: Duration = Duration::from_secs(5);

fn err(e: impl std::fmt::Display) -> CmsError {
    CmsError::Storage(format!("valkey: {e}"))
}

/// [Valkey](https://valkey.io) store (any Redis-compatible server: Redis,
/// KeyDB, Dragonfly, managed services). URL: `redis://[user:pass@]host:6379/0`,
/// or `rediss://` for TLS.
///
/// Connects lazily: Tess starts and serves requests (uncached) while the
/// server is unreachable, and uses it as soon as it answers.
pub struct ValkeyStore {
    client: redis::Client,
    conn: Mutex<Connection>,
}

enum Connection {
    Idle,
    Ready(ConnectionManager),
    Failed(Instant),
}

impl ValkeyStore {
    pub fn new(url: &str) -> Result<Self> {
        if url.starts_with("rediss://") {
            // Several rustls crypto providers are compiled in: pick one (kept
            // if something else installed one first).
            let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        }
        let client = redis::Client::open(url).map_err(err)?;
        Ok(ValkeyStore { client, conn: Mutex::new(Connection::Idle) })
    }

    async fn connection(&self) -> Result<ConnectionManager> {
        let mut conn = self.conn.lock().await;
        match &*conn {
            Connection::Ready(c) => return Ok(c.clone()),
            Connection::Failed(at) if at.elapsed() < RETRY_AFTER => {
                return Err(err("unreachable, retrying shortly"));
            }
            _ => {}
        }
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(Duration::from_secs(2)))
            .set_response_timeout(Some(Duration::from_secs(1)))
            .set_number_of_retries(2);
        match ConnectionManager::new_with_config(self.client.clone(), config).await {
            Ok(c) => {
                tracing::info!("connected to the cache server");
                *conn = Connection::Ready(c.clone());
                Ok(c)
            }
            Err(e) => {
                *conn = Connection::Failed(Instant::now());
                Err(err(e))
            }
        }
    }

    async fn query<T: redis::FromRedisValue>(&self, cmd: &redis::Cmd) -> Result<T> {
        let mut conn = self.connection().await?;
        match cmd.query_async(&mut conn).await {
            Ok(v) => Ok(v),
            Err(e) => {
                // A hung or lost server: skip it for a while instead of making
                // every request wait for the timeout.
                if e.is_timeout() || e.is_io_error() || e.is_connection_dropped() || e.is_unrecoverable_error() {
                    *self.conn.lock().await = Connection::Failed(Instant::now());
                }
                Err(err(e))
            }
        }
    }
}

#[async_trait]
impl CacheStore for ValkeyStore {
    fn name(&self) -> &'static str {
        "valkey"
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.query(redis::cmd("GET").arg(key)).await
    }

    async fn set(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()> {
        let ms = u64::try_from(ttl.as_millis()).unwrap_or(u64::MAX).max(1);
        self.query(redis::cmd("SET").arg(key).arg(value).arg("PX").arg(ms)).await
    }

    async fn counter(&self, key: &str) -> Result<u64> {
        Ok(self.query::<Option<u64>>(redis::cmd("GET").arg(key)).await?.unwrap_or(0))
    }

    async fn increment(&self, key: &str) -> Result<u64> {
        self.query(redis::cmd("INCR").arg(key)).await
    }

    async fn publish(&self, channel: &str, message: &str) -> Result<()> {
        self.query::<i64>(redis::cmd("PUBLISH").arg(channel).arg(message)).await.map(|_| ())
    }

    fn subscribe(&self, channel: &str, on_message: MessageFn) {
        let client = self.client.clone();
        let channel = channel.to_string();
        tokio::spawn(async move {
            let mut first = true;
            loop {
                match client.get_async_pubsub().await {
                    Ok(mut pubsub) => match pubsub.subscribe(&channel).await {
                        Ok(()) => {
                            if !first {
                                on_message(""); // messages may have been missed meanwhile
                            }
                            let mut messages = pubsub.into_on_message();
                            while let Some(msg) = messages.next().await {
                                on_message(&msg.get_payload::<String>().unwrap_or_default());
                            }
                            tracing::warn!(channel, "cache subscription closed, reconnecting");
                        }
                        Err(e) => tracing::warn!(channel, error = %e, "cache subscription failed"),
                    },
                    Err(e) => tracing::debug!(channel, error = %e, "cache server unreachable"),
                }
                first = false;
                tokio::time::sleep(RETRY_AFTER).await;
            }
        });
    }
}
