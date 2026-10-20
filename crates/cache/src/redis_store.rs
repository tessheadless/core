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

fn err(e: impl std::fmt::Display) -> CmsError {
    CmsError::Storage(format!("cache server: {e}"))
}

/// Start of a missing counter: now, in milliseconds.
fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_millis() as u64)
}

/// After a failed connection, requests skip the cache this long before the
/// next attempt (so a down server costs nothing per request).
const RETRY_AFTER: Duration = Duration::from_secs(5);

/// Store on any server speaking the Redis protocol: [Valkey](https://valkey.io),
/// [Redis](https://redis.io), [KeyDB](https://docs.keydb.dev),
/// [Dragonfly](https://www.dragonflydb.io), and managed services (Amazon
/// ElastiCache and MemoryDB, Azure Cache for Redis, Google Memorystore, Redis
/// Cloud, Upstash…). URL: `redis://[user:password@]host:6379/0`, or
/// `rediss://` for TLS. Only plain commands are used (`GET`, `SET … PX`,
/// `INCR`, `PUBLISH`, `SUBSCRIBE`), with a single primary endpoint (no
/// cluster mode).
///
/// Connects lazily: Tess starts and serves requests (uncached) while the
/// server is unreachable, and uses it as soon as it answers.
pub struct RedisStore {
    client: redis::Client,
    conn: Mutex<Connection>,
    /// What the server is (`valkey`, `redis`…), for logs and `/health`.
    name: &'static str,
}

enum Connection {
    Idle,
    Ready(ConnectionManager),
    Failed(Instant),
}

impl RedisStore {
    /// `name` says which server it is (`valkey`, `redis`, `keydb`, `dragonfly`):
    /// they all use the same client.
    pub fn new(url: &str, name: &'static str) -> Result<Self> {
        if url.starts_with("rediss://") {
            // Several rustls crypto providers are compiled in: pick one (kept
            // if something else installed one first).
            let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        }
        let client = redis::Client::open(url).map_err(err)?;
        Ok(RedisStore {
            client,
            conn: Mutex::new(Connection::Idle),
            name,
        })
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
        let res = cmd.query_async(&mut conn).await;
        self.check(res).await
    }

    async fn pipeline<T: redis::FromRedisValue>(&self, pipe: &redis::Pipeline) -> Result<T> {
        let mut conn = self.connection().await?;
        let res = pipe.query_async(&mut conn).await;
        self.check(res).await
    }

    async fn check<T>(&self, res: redis::RedisResult<T>) -> Result<T> {
        res.map_err(|e| {
            // A hung or lost server: skip it for a while instead of making
            // every request wait for the timeout.
            if (e.is_timeout()
                || e.is_io_error()
                || e.is_connection_dropped()
                || e.is_unrecoverable_error())
                && let Ok(mut conn) = self.conn.try_lock()
            {
                *conn = Connection::Failed(Instant::now());
            }
            err(e)
        })
    }

    /// Runs `command key` on a counter, creating it first when missing.
    async fn seeded(&self, command: &str, key: &str) -> Result<u64> {
        let pipe = redis::pipe()
            .cmd("SET")
            .arg(key)
            .arg(seed())
            .arg("NX")
            .ignore()
            .cmd(command)
            .arg(key)
            .clone();
        let (value,): (u64,) = self.pipeline(&pipe).await?;
        Ok(value)
    }
}

#[async_trait]
impl CacheStore for RedisStore {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.query(redis::cmd("GET").arg(key)).await
    }

    async fn set(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()> {
        let ms = u64::try_from(ttl.as_millis()).unwrap_or(u64::MAX).max(1);
        self.query(redis::cmd("SET").arg(key).arg(value).arg("PX").arg(ms))
            .await
    }

    // A counter missing from the server (never set, or evicted under memory
    // pressure: Dragonfly's cache mode or `allkeys-*` policies can do that)
    // restarts from the current time in milliseconds, never from 0: a new
    // generation can't match entries cached under an older one.
    async fn counter(&self, key: &str) -> Result<u64> {
        self.seeded("GET", key).await
    }

    async fn increment(&self, key: &str) -> Result<u64> {
        self.seeded("INCR", key).await
    }

    async fn publish(&self, channel: &str, message: &str) -> Result<()> {
        self.query::<i64>(redis::cmd("PUBLISH").arg(channel).arg(message))
            .await
            .map(|_| ())
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
