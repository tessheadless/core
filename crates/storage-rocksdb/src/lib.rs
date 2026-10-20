// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Embedded RocksDB backend: zero external services, ideal for a single
//! Docker container with a persistent volume. Not suitable for AWS Lambda or
//! for several server replicas (a RocksDB directory has a single writer).
//!
//! Key layout:
//! - `ct\0{apiId}`        -> content type JSON
//! - `e\0{apiId}\0{uuid}` -> entry JSON
//!
//! Queries scan the content type's key prefix and evaluate filters in memory.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use rocksdb::{DB, Direction, IteratorMode, Options, WriteBatch};
use tess_core::query::execute_in_memory;
use tess_core::{CmsError, ContentType, Entry, EntryQuery, Page, Result, Storage};
use uuid::Uuid;

pub struct RocksDbStorage {
    db: Arc<DB>,
}

impl RocksDbStorage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path).map_err(CmsError::storage)?;
        Ok(RocksDbStorage { db: Arc::new(db) })
    }

    /// Runs blocking RocksDB work off the async runtime.
    async fn blocking<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&DB) -> Result<T> + Send + 'static,
    {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || f(&db))
            .await
            .map_err(CmsError::storage)?
    }
}

fn ct_key(api_id: &str) -> Vec<u8> {
    format!("ct\0{api_id}").into_bytes()
}

fn entries_prefix(api_id: &str) -> Vec<u8> {
    format!("e\0{api_id}\0").into_bytes()
}

fn entry_key(api_id: &str, id: Uuid) -> Vec<u8> {
    format!("e\0{api_id}\0{id}").into_bytes()
}

type KvPairs = Vec<(Box<[u8]>, Box<[u8]>)>;

/// Iterates `(key, value)` pairs under a prefix.
fn scan_prefix(db: &DB, prefix: &[u8]) -> Result<KvPairs> {
    let mut out = Vec::new();
    for item in db.iterator(IteratorMode::From(prefix, Direction::Forward)) {
        let (k, v) = item.map_err(CmsError::storage)?;
        if !k.starts_with(prefix) {
            break;
        }
        out.push((k, v));
    }
    Ok(out)
}

fn setting_key(key: &str) -> Vec<u8> {
    format!("s\0{key}").into_bytes()
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).map_err(CmsError::storage)
}

fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(CmsError::storage)
}

#[async_trait]
impl Storage for RocksDbStorage {
    fn name(&self) -> &'static str {
        "rocksdb"
    }

    async fn list_content_types(&self) -> Result<Vec<ContentType>> {
        self.blocking(|db| {
            scan_prefix(db, b"ct\0")?
                .iter()
                .map(|(_, v)| decode(v))
                .collect()
        })
        .await
    }

    async fn get_content_type(&self, api_id: &str) -> Result<Option<ContentType>> {
        let key = ct_key(api_id);
        self.blocking(move |db| {
            db.get_pinned(key)
                .map_err(CmsError::storage)?
                .map(|v| decode(&v))
                .transpose()
        })
        .await
    }

    async fn put_content_type(&self, content_type: &ContentType) -> Result<()> {
        let key = ct_key(&content_type.api_id);
        let value = encode(content_type)?;
        self.blocking(move |db| db.put(key, value).map_err(CmsError::storage))
            .await
    }

    async fn delete_content_type(&self, api_id: &str) -> Result<()> {
        let key = ct_key(api_id);
        let prefix = entries_prefix(api_id);
        self.blocking(move |db| {
            let mut batch = WriteBatch::default();
            batch.delete(key);
            for (k, _) in scan_prefix(db, &prefix)? {
                batch.delete(k);
            }
            db.write(batch).map_err(CmsError::storage)
        })
        .await
    }

    async fn get_entry(&self, content_type: &str, id: Uuid) -> Result<Option<Entry>> {
        let key = entry_key(content_type, id);
        self.blocking(move |db| {
            db.get_pinned(key)
                .map_err(CmsError::storage)?
                .map(|v| decode(&v))
                .transpose()
        })
        .await
    }

    async fn put_entry(&self, entry: &Entry) -> Result<()> {
        let key = entry_key(&entry.content_type, entry.id);
        let value = encode(entry)?;
        self.blocking(move |db| db.put(key, value).map_err(CmsError::storage))
            .await
    }

    async fn delete_entry(&self, content_type: &str, id: Uuid) -> Result<bool> {
        let key = entry_key(content_type, id);
        self.blocking(move |db| {
            let existed = db.get_pinned(&key).map_err(CmsError::storage)?.is_some();
            db.delete(key).map_err(CmsError::storage)?;
            Ok(existed)
        })
        .await
    }

    async fn query_entries(&self, content_type: &str, query: &EntryQuery) -> Result<Page> {
        let prefix = entries_prefix(content_type);
        let query = query.clone();
        self.blocking(move |db| {
            let entries = scan_prefix(db, &prefix)?
                .iter()
                .map(|(_, v)| decode::<Entry>(v))
                .collect::<Result<Vec<_>>>()?;
            Ok(execute_in_memory(entries, &query))
        })
        .await
    }

    async fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let key = setting_key(key);
        self.blocking(move |db| {
            db.get_pinned(key)
                .map_err(CmsError::storage)?
                .map(|v| decode(&v))
                .transpose()
        })
        .await
    }

    async fn put_setting(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        let key = setting_key(key);
        let value = encode(value)?;
        self.blocking(move |db| db.put(key, value).map_err(CmsError::storage))
            .await
    }

    async fn delete_setting(&self, key: &str) -> Result<bool> {
        let key = setting_key(key);
        self.blocking(move |db| {
            let existed = db.get_pinned(&key).map_err(CmsError::storage)?.is_some();
            db.delete(key).map_err(CmsError::storage)?;
            Ok(existed)
        })
        .await
    }

    async fn list_settings(&self, prefix: &str) -> Result<Vec<(String, serde_json::Value)>> {
        let prefix = setting_key(prefix);
        self.blocking(move |db| {
            scan_prefix(db, &prefix)?
                .iter()
                .map(|(k, v)| Ok((String::from_utf8_lossy(&k[2..]).into_owned(), decode(v)?)))
                .collect()
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn conformance() {
        let dir = tempfile::tempdir().unwrap();
        let storage = RocksDbStorage::open(dir.path()).unwrap();
        tess_core::testkit::run_conformance(Arc::new(storage)).await;
    }
}
