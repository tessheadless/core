// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use crate::error::Result;
use crate::model::{ContentType, Entry};
use crate::query::{EntryQuery, Page};

/// Persistence contract implemented by every database backend.
///
/// Implementations are deliberately dumb: validation, defaults and
/// referential checks live in [`crate::Cms`], so every backend behaves the
/// same. `crate::testkit` (feature `testkit`) holds the conformance suite each
/// backend must pass.
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    /// Short backend name, for logs and the health endpoint.
    fn name(&self) -> &'static str;

    /// Creates tables/collections/indexes if needed. Must be idempotent and
    /// safe to run concurrently (several Lambda cold starts at once).
    async fn migrate(&self) -> Result<()> {
        Ok(())
    }

    async fn list_content_types(&self) -> Result<Vec<ContentType>>;
    async fn get_content_type(&self, api_id: &str) -> Result<Option<ContentType>>;
    /// Inserts or replaces a content type.
    async fn put_content_type(&self, content_type: &ContentType) -> Result<()>;
    /// Deletes a content type together with all of its entries.
    async fn delete_content_type(&self, api_id: &str) -> Result<()>;

    async fn get_entry(&self, content_type: &str, id: Uuid) -> Result<Option<Entry>>;
    /// Inserts or replaces an entry (last write wins).
    async fn put_entry(&self, entry: &Entry) -> Result<()>;
    /// Returns whether an entry was deleted.
    async fn delete_entry(&self, content_type: &str, id: Uuid) -> Result<bool>;
    /// Filters, sorts and paginates entries of one content type. Entries with
    /// no data for `query.stage` (unpublished entries) are never returned.
    async fn query_entries(&self, content_type: &str, query: &EntryQuery) -> Result<Page>;

    /// Small JSON documents holding system state (plugin registry, ...).
    /// Keys are `/`-separated paths; keep values well under 300 KB (DynamoDB
    /// item limit). Large binaries belong in the blob store.
    async fn get_setting(&self, key: &str) -> Result<Option<Value>>;
    async fn put_setting(&self, key: &str, value: &Value) -> Result<()>;
    /// Returns whether a setting was deleted.
    async fn delete_setting(&self, key: &str) -> Result<bool>;
    /// Settings whose key starts with `prefix`, sorted by key.
    async fn list_settings(&self, prefix: &str) -> Result<Vec<(String, Value)>>;
}
