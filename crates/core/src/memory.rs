// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Volatile in-process backend, for tests and quick local experiments.

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::Result;
use crate::model::{ContentType, Entry};
use crate::query::{EntryQuery, Page, execute_in_memory};
use crate::storage::Storage;

#[derive(Default)]
pub struct MemoryStorage {
    content_types: RwLock<BTreeMap<String, ContentType>>,
    entries: RwLock<BTreeMap<(String, Uuid), Entry>>,
    settings: RwLock<BTreeMap<String, Value>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    fn name(&self) -> &'static str {
        "memory"
    }

    async fn list_content_types(&self) -> Result<Vec<ContentType>> {
        Ok(self.content_types.read().await.values().cloned().collect())
    }

    async fn get_content_type(&self, api_id: &str) -> Result<Option<ContentType>> {
        Ok(self.content_types.read().await.get(api_id).cloned())
    }

    async fn put_content_type(&self, content_type: &ContentType) -> Result<()> {
        self.content_types
            .write()
            .await
            .insert(content_type.api_id.clone(), content_type.clone());
        Ok(())
    }

    async fn delete_content_type(&self, api_id: &str) -> Result<()> {
        self.content_types.write().await.remove(api_id);
        self.entries.write().await.retain(|(ct, _), _| ct != api_id);
        Ok(())
    }

    async fn get_entry(&self, content_type: &str, id: Uuid) -> Result<Option<Entry>> {
        Ok(self
            .entries
            .read()
            .await
            .get(&(content_type.to_string(), id))
            .cloned())
    }

    async fn put_entry(&self, entry: &Entry) -> Result<()> {
        self.entries
            .write()
            .await
            .insert((entry.content_type.clone(), entry.id), entry.clone());
        Ok(())
    }

    async fn delete_entry(&self, content_type: &str, id: Uuid) -> Result<bool> {
        Ok(self
            .entries
            .write()
            .await
            .remove(&(content_type.to_string(), id))
            .is_some())
    }

    async fn query_entries(&self, content_type: &str, query: &EntryQuery) -> Result<Page> {
        let entries = self.entries.read().await;
        let of_type = entries
            .range(
                (content_type.to_string(), Uuid::nil())..=(content_type.to_string(), Uuid::max()),
            )
            .map(|(_, e)| e.clone());
        Ok(execute_in_memory(of_type, query))
    }

    async fn get_setting(&self, key: &str) -> Result<Option<Value>> {
        Ok(self.settings.read().await.get(key).cloned())
    }

    async fn put_setting(&self, key: &str, value: &Value) -> Result<()> {
        self.settings
            .write()
            .await
            .insert(key.to_string(), value.clone());
        Ok(())
    }

    async fn delete_setting(&self, key: &str) -> Result<bool> {
        Ok(self.settings.write().await.remove(key).is_some())
    }

    async fn list_settings(&self, prefix: &str) -> Result<Vec<(String, Value)>> {
        let settings = self.settings.read().await;
        Ok(settings
            .range(prefix.to_string()..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    #[tokio::test]
    async fn conformance() {
        crate::testkit::run_conformance(Arc::new(super::MemoryStorage::new())).await;
    }
}
