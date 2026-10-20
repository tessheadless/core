// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Full-text search. Entries are pushed to a [`SearchEngine`] by
//! [`SearchService`], a built-in hook, whenever they change; queries return
//! entry ids that are then loaded from the primary database, so search results
//! always reflect stored content and permissions.
//!
//! Search is opt-in per content type: an Admin enables it and picks the
//! indexed fields ([`ModelSearchSettings`], kept in the settings store). The
//! engine schema of each field follows from its type ([`IndexKind`]).
//!
//! Implementations: [`TypesenseEngine`] (Typesense), and [`MemoryEngine`] for
//! tests. Other engines (Meilisearch, OpenSearch, ...) implement the trait.

mod memory;
mod typesense;

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tess_core::query::EntryQuery;
use tess_core::{
    Actor, Cms, CmsError, ContentType, Entry, EntryAction, EntryEvent, FieldDef, FieldType, Hook, Result, Stage,
    Storage,
};
use uuid::Uuid;

pub use memory::MemoryEngine;
pub use typesense::{TypesenseConfig, TypesenseEngine};

/// Settings key prefix: `search/models/<ApiId>`.
pub const SETTINGS_PREFIX: &str = "search/models/";

/// Search settings of one content type, set by an Admin.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSearchSettings {
    pub enabled: bool,
    /// Indexed fields, in model order. Kept when search is disabled.
    #[serde(default)]
    pub fields: Vec<String>,
}

/// How a field is indexed; engines map it to their own field types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKind {
    /// Full-text searched (`TEXT`, `MARKDOWN`).
    Text,
    /// Exact value, filterable and faceted (`ENUM`, `UUID`, single `RELATION`).
    Keyword,
    /// List of exact values (`RELATION` with many).
    Keywords,
    Int,
    Float,
    Bool,
    /// Unix seconds, sortable (`DATE` at midnight UTC, `DATETIME`).
    Timestamp,
}

impl IndexKind {
    pub fn as_str(self) -> &'static str {
        match self {
            IndexKind::Text => "TEXT",
            IndexKind::Keyword => "KEYWORD",
            IndexKind::Keywords => "KEYWORDS",
            IndexKind::Int => "INT",
            IndexKind::Float => "FLOAT",
            IndexKind::Bool => "BOOL",
            IndexKind::Timestamp => "TIMESTAMP",
        }
    }
}

/// `None` for types that cannot be indexed: JSON, components, dynamic zones
/// and passwords (never indexed).
pub fn index_kind(field_type: &FieldType) -> Option<IndexKind> {
    Some(match field_type {
        FieldType::Text { .. } | FieldType::Markdown => IndexKind::Text,
        FieldType::Enum { .. } | FieldType::Uuid | FieldType::Relation { many: false, .. } => IndexKind::Keyword,
        FieldType::Relation { many: true, .. } => IndexKind::Keywords,
        FieldType::Int => IndexKind::Int,
        FieldType::Float => IndexKind::Float,
        FieldType::Boolean => IndexKind::Bool,
        FieldType::Date | FieldType::DateTime => IndexKind::Timestamp,
        FieldType::Json | FieldType::Password { .. } | FieldType::Component { .. } | FieldType::DynamicZone { .. } => {
            return None;
        }
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexField {
    pub name: String,
    pub kind: IndexKind,
}

/// What an engine indexes for one content type.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchIndex {
    pub content_type: String,
    pub fields: Vec<IndexField>,
}

impl SearchIndex {
    /// The index of `ct` under `settings`; `None` when search is disabled or
    /// no full-text field is selected. Fields that no longer exist, or whose
    /// type cannot be indexed, are skipped.
    pub fn new(ct: &ContentType, settings: &ModelSearchSettings) -> Option<Self> {
        if !settings.enabled {
            return None;
        }
        let fields: Vec<IndexField> = ct
            .fields
            .iter()
            .filter(|f| settings.fields.contains(&f.name))
            .filter_map(|f| Some(IndexField { name: f.name.clone(), kind: index_kind(&f.field_type)? }))
            .collect();
        fields.iter().any(|f| f.kind == IndexKind::Text).then(|| SearchIndex { content_type: ct.api_id.clone(), fields })
    }

    /// Fields searched by queries.
    pub fn text_fields(&self) -> Vec<&str> {
        self.fields.iter().filter(|f| f.kind == IndexKind::Text).map(|f| f.name.as_str()).collect()
    }
}

/// A document as sent to the engine: one per (entry, stage).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchDoc {
    pub entry_id: Uuid,
    pub stage: Stage,
    /// Indexed fields, converted to their [`IndexKind`].
    pub fields: Map<String, Value>,
}

impl SearchDoc {
    pub fn doc_id(&self) -> String {
        doc_id(self.entry_id, self.stage)
    }
}

pub fn doc_id(entry_id: Uuid, stage: Stage) -> String {
    let s = match stage {
        Stage::Draft => "draft",
        Stage::Published => "published",
    };
    format!("{s}_{entry_id}")
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchRequest {
    pub text: String,
    pub stage: Stage,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SearchHits {
    /// Entry ids, best match first.
    pub ids: Vec<Uuid>,
    pub total: u64,
}

#[async_trait]
pub trait SearchEngine: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    /// The engine's own definition of a field (shown to Admins), if it has one.
    fn field_config(&self, _field: &IndexField) -> Option<Value> {
        None
    }
    /// (Re)creates the index of a content type, dropping existing documents.
    async fn reset_index(&self, index: &SearchIndex) -> Result<()>;
    async fn drop_index(&self, content_type: &str) -> Result<()>;
    async fn upsert(&self, index: &SearchIndex, docs: Vec<SearchDoc>) -> Result<()>;
    async fn delete(&self, content_type: &str, doc_ids: Vec<String>) -> Result<()>;
    async fn search(&self, index: &SearchIndex, request: &SearchRequest) -> Result<SearchHits>;
}

/// The stored value converted for the index; `None` skips it.
fn index_value(kind: IndexKind, v: &Value) -> Option<Value> {
    match kind {
        IndexKind::Text | IndexKind::Keyword => v.is_string().then(|| v.clone()),
        IndexKind::Keywords => {
            let ids: Vec<Value> = v.as_array()?.iter().filter(|x| x.is_string()).cloned().collect();
            Some(Value::Array(ids))
        }
        IndexKind::Int => v.is_i64().then(|| v.clone()),
        IndexKind::Float => v.as_f64().map(Value::from),
        IndexKind::Bool => v.is_boolean().then(|| v.clone()),
        IndexKind::Timestamp => {
            let s = v.as_str()?;
            let secs = match DateTime::parse_from_rfc3339(s) {
                Ok(t) => t.timestamp(),
                Err(_) => NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?.and_hms_opt(0, 0, 0)?.and_utc().timestamp(),
            };
            Some(Value::from(secs))
        }
    }
}

fn docs_for(index: &SearchIndex, entry: &Entry) -> Vec<SearchDoc> {
    [Stage::Draft, Stage::Published]
        .into_iter()
        .filter_map(|stage| {
            let data = entry.data(stage)?;
            let fields = index
                .fields
                .iter()
                .filter_map(|f| Some((f.name.clone(), index_value(f.kind, data.get(&f.name)?)?)))
                .collect();
            Some(SearchDoc { entry_id: entry.id, stage, fields })
        })
        .collect()
}

fn settings_key(content_type: &str) -> String {
    format!("{SETTINGS_PREFIX}{content_type}")
}

/// Keeps the search engine in sync and runs searches.
#[derive(Clone)]
pub struct SearchService {
    engine: Arc<dyn SearchEngine>,
    storage: Arc<dyn Storage>,
}

impl SearchService {
    pub fn new(engine: Arc<dyn SearchEngine>, storage: Arc<dyn Storage>) -> Self {
        SearchService { engine, storage }
    }

    pub fn engine(&self) -> &Arc<dyn SearchEngine> {
        &self.engine
    }

    /// Registers the indexing hook.
    pub fn install(&self, cms: &Cms) {
        cms.hooks().replace_prefixed("builtin:search", vec![Arc::new(SearchHook(self.clone()))]);
    }

    /// Settings of one content type (disabled when never set).
    pub async fn settings(&self, content_type: &str) -> Result<ModelSearchSettings> {
        match self.storage.get_setting(&settings_key(content_type)).await? {
            Some(v) => serde_json::from_value(v).map_err(CmsError::storage),
            None => Ok(ModelSearchSettings::default()),
        }
    }

    /// Settings of every content type that has some, by api id.
    pub async fn all_settings(&self) -> Result<BTreeMap<String, ModelSearchSettings>> {
        self.storage
            .list_settings(SETTINGS_PREFIX)
            .await?
            .into_iter()
            .map(|(key, v)| {
                let api_id = key.strip_prefix(SETTINGS_PREFIX).unwrap_or(&key).to_string();
                Ok((api_id, serde_json::from_value(v).map_err(CmsError::storage)?))
            })
            .collect()
    }

    /// The current index of `ct`, `None` when it is not searchable.
    pub async fn index_for(&self, ct: &ContentType) -> Result<Option<SearchIndex>> {
        Ok(SearchIndex::new(ct, &self.settings(&ct.api_id).await?))
    }

    /// Enables or disables search on `ct` with these fields (Admins), then
    /// rebuilds (or drops) its index. Returns the saved settings and the
    /// number of entries indexed.
    pub async fn update_settings(
        &self,
        ct: &ContentType,
        enabled: bool,
        fields: &[String],
        actor: &Actor,
    ) -> Result<(ModelSearchSettings, u64)> {
        actor.require_admin()?;
        for name in fields {
            let field = ct
                .fields
                .iter()
                .find(|f| f.name == *name)
                .ok_or_else(|| CmsError::validation(format!("`{}` has no field `{name}`", ct.api_id)))?;
            if index_kind(&field.field_type).is_none() {
                return Err(CmsError::validation(format!("field `{name}`: this type cannot be indexed")));
            }
        }
        // Model order, without duplicates.
        let fields = ct.fields.iter().filter(|f| fields.contains(&f.name)).map(|f| f.name.clone()).collect();
        let settings = ModelSearchSettings { enabled, fields };
        if enabled && SearchIndex::new(ct, &settings).is_none() {
            return Err(CmsError::validation("select at least one TEXT or MARKDOWN field to search"));
        }
        let value = serde_json::to_value(&settings).map_err(CmsError::storage)?;
        self.storage.put_setting(&settings_key(&ct.api_id), &value).await?;
        let count = self.reindex(ct).await?;
        Ok((settings, count))
    }

    /// Rebuilds the index of one content type from the database (drops it
    /// when the type is not searchable).
    pub async fn reindex(&self, ct: &ContentType) -> Result<u64> {
        let Some(index) = self.index_for(ct).await? else {
            self.engine.drop_index(&ct.api_id).await?;
            return Ok(0);
        };
        self.engine.reset_index(&index).await?;
        let mut count = 0;
        let mut q = EntryQuery::new(Stage::Draft);
        q.limit = 500;
        loop {
            let page = self.storage.query_entries(&ct.api_id, &q).await?;
            let docs: Vec<_> = page.items.iter().flat_map(|e| docs_for(&index, e)).collect();
            count += page.items.len() as u64;
            if !docs.is_empty() {
                self.engine.upsert(&index, docs).await?;
            }
            q.offset += page.items.len();
            if page.items.is_empty() || q.offset as u64 >= page.total {
                return Ok(count);
            }
        }
    }

    /// Ids of matching entries, best first.
    pub async fn search(&self, ct: &ContentType, request: &SearchRequest) -> Result<SearchHits> {
        match self.index_for(ct).await? {
            Some(index) => self.engine.search(&index, request).await,
            None => Ok(SearchHits::default()),
        }
    }

    async fn sync_entry(&self, event: &EntryEvent) -> Result<()> {
        let Some(ct) = self.storage.get_content_type(&event.content_type).await? else {
            return Ok(());
        };
        let Some(index) = self.index_for(&ct).await? else {
            return Ok(());
        };
        let id = event.entry.as_ref().or(event.previous.as_ref()).map(|e| e.id).unwrap_or_default();
        let docs = event.entry.as_ref().map(|e| docs_for(&index, e)).unwrap_or_default();
        // Stages that no longer exist (unpublish, delete) are removed.
        let stale: Vec<String> = [Stage::Draft, Stage::Published]
            .into_iter()
            .filter(|s| !docs.iter().any(|d| d.stage == *s))
            .map(|s| doc_id(id, s))
            .collect();
        if !docs.is_empty() {
            self.engine.upsert(&index, docs).await?;
        }
        if !stale.is_empty() && event.action != EntryAction::Create {
            self.engine.delete(&ct.api_id, stale).await?;
        }
        Ok(())
    }

    async fn content_type_changed(&self, ct: &ContentType, deleted: bool) -> Result<()> {
        if deleted {
            self.engine.drop_index(&ct.api_id).await?;
            self.storage.delete_setting(&settings_key(&ct.api_id)).await?;
            return Ok(());
        }
        self.reindex(ct).await.map(|_| ())
    }
}

/// Fields of `ct` an Admin can pick, with how each one would be indexed.
pub fn indexable_fields(ct: &ContentType) -> impl Iterator<Item = (&FieldDef, Option<IndexKind>)> {
    ct.fields.iter().map(|f| (f, index_kind(&f.field_type)))
}

struct SearchHook(SearchService);

#[async_trait]
impl Hook for SearchHook {
    fn name(&self) -> &str {
        "builtin:search"
    }

    async fn after_write(&self, event: &EntryEvent) {
        if let Err(e) = self.0.sync_entry(event).await {
            tracing::warn!(content_type = %event.content_type, error = %e, "search indexing failed");
        }
    }

    async fn after_content_type_change(&self, ct: &ContentType, deleted: bool) {
        if let Err(e) = self.0.content_type_changed(ct, deleted).await {
            tracing::warn!(content_type = %ct.api_id, error = %e, "search index update failed");
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use serde_json::json;

    use super::*;
    use tess_core::memory::MemoryStorage;
    use tess_core::{ContentTypeDef, Role};

    pub(crate) fn data(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn names(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    /// Shared scenario run against every engine.
    pub(crate) async fn scenario(engine: Arc<dyn SearchEngine>) {
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let cms = Cms::new(storage.clone());
        let search = SearchService::new(engine, storage);
        search.install(&cms);
        let sys = Actor::system();
        let f = |name: &str, field_type| FieldDef { name: name.into(), field_type, required: false, description: None };
        let ct = cms
            .create_content_type(
                ContentTypeDef {
                    workflow: false,
                    api_id: "SearchTkArticle".into(),
                    plural_api_id: None,
                    description: None,
                    fields: vec![
                        f("title", FieldType::text()),
                        f("body", FieldType::text()),
                        f("views", FieldType::Int),
                        f("day", FieldType::Date),
                        f("meta", FieldType::Json),
                    ],
                },
                &sys,
            )
            .await
            .unwrap();
        let q = |text: &str, stage| SearchRequest { text: text.into(), stage, limit: 10, offset: 0 };

        let rust = cms
            .create_entry(
                &ct.api_id,
                data(json!({"title": "Rust ownership", "body": "Borrowing explained", "views": 3, "day": "2026-01-31", "meta": {"x": 1}})),
                &sys,
            )
            .await
            .unwrap();
        let cms_doc =
            cms.create_entry(&ct.api_id, data(json!({"title": "Headless CMS", "body": "Content with GraphQL"})), &sys).await.unwrap();
        cms.publish_entry(&ct.api_id, rust.id, &sys).await.unwrap();

        // Opt-in: nothing is searchable before an Admin enables it.
        assert!(search.index_for(&ct).await.unwrap().is_none());
        assert_eq!(search.search(&ct, &q("rust", Stage::Draft)).await.unwrap().total, 0);
        let editor = Actor { id: None, name: "ed".into(), role: Role::Contributor, scope: vec![] };
        assert!(search.update_settings(&ct, true, &names(&["title"]), &editor).await.is_err(), "admins only");
        assert!(search.update_settings(&ct, true, &names(&["views"]), &sys).await.is_err(), "needs a text field");
        assert!(search.update_settings(&ct, true, &names(&["title", "meta"]), &sys).await.is_err(), "JSON");
        assert!(search.update_settings(&ct, true, &names(&["nope"]), &sys).await.is_err());

        // Only `title` is indexed (and searched) at first.
        let (saved, count) = search.update_settings(&ct, true, &names(&["views", "title", "title"]), &sys).await.unwrap();
        assert_eq!((saved.fields, count), (names(&["title", "views"]), 2));
        assert_eq!(search.search(&ct, &q("borrowing", Stage::Draft)).await.unwrap().total, 0);
        assert_eq!(search.search(&ct, &q("ownership", Stage::Draft)).await.unwrap().ids, vec![rust.id]);
        search.update_settings(&ct, true, &names(&["title", "body", "views", "day"]), &sys).await.unwrap();

        let hits = search.search(&ct, &q("borrowing", Stage::Draft)).await.unwrap();
        assert_eq!(hits.ids, vec![rust.id]);
        assert_eq!(search.search(&ct, &q("graphql", Stage::Published)).await.unwrap().total, 0, "drafts are not public");
        assert_eq!(search.search(&ct, &q("graphql", Stage::Draft)).await.unwrap().ids, vec![cms_doc.id]);

        // Updates replace the indexed draft but not the published version.
        cms.update_entry(&ct.api_id, rust.id, data(json!({"title": "Rust lifetimes"})), &sys).await.unwrap();
        assert_eq!(search.search(&ct, &q("lifetimes", Stage::Draft)).await.unwrap().total, 1);
        assert_eq!(search.search(&ct, &q("lifetimes", Stage::Published)).await.unwrap().total, 0);
        assert_eq!(search.search(&ct, &q("ownership", Stage::Published)).await.unwrap().total, 1);

        cms.unpublish_entry(&ct.api_id, rust.id, &sys).await.unwrap();
        assert_eq!(search.search(&ct, &q("ownership", Stage::Published)).await.unwrap().total, 0);
        cms.delete_entry(&ct.api_id, rust.id, &sys).await.unwrap();
        assert_eq!(search.search(&ct, &q("lifetimes", Stage::Draft)).await.unwrap().total, 0);

        // Reindex from the database.
        let index = search.index_for(&ct).await.unwrap().unwrap();
        search.engine().reset_index(&index).await.unwrap();
        assert_eq!(search.search(&ct, &q("graphql", Stage::Draft)).await.unwrap().total, 0);
        assert_eq!(search.reindex(&ct).await.unwrap(), 1);
        assert_eq!(search.search(&ct, &q("graphql", Stage::Draft)).await.unwrap().total, 1);

        // Disabling drops the index but remembers the fields.
        let (saved, _) = search.update_settings(&ct, false, &names(&["title", "body"]), &sys).await.unwrap();
        assert_eq!(saved.fields, names(&["title", "body"]));
        assert_eq!(search.search(&ct, &q("graphql", Stage::Draft)).await.unwrap().total, 0);

        cms.delete_content_type(&ct.api_id, &sys).await.unwrap();
        assert!(search.all_settings().await.unwrap().is_empty(), "settings removed with the type");
    }

    #[tokio::test]
    async fn memory_engine() {
        scenario(Arc::new(MemoryEngine::default())).await;
    }

    #[test]
    fn values_follow_the_field_type() {
        assert_eq!(index_value(IndexKind::Timestamp, &json!("1970-01-02")), Some(json!(86400)));
        assert_eq!(index_value(IndexKind::Timestamp, &json!("1970-01-01T00:01:00.000Z")), Some(json!(60)));
        assert_eq!(index_value(IndexKind::Timestamp, &json!("soon")), None);
        assert_eq!(index_value(IndexKind::Float, &json!(2)), Some(json!(2.0)));
        assert_eq!(index_value(IndexKind::Int, &json!(2.5)), None);
        assert_eq!(index_value(IndexKind::Keywords, &json!(["a", 1, "b"])), Some(json!(["a", "b"])));
        assert_eq!(index_kind(&FieldType::Password { min_length: 8 }), None);
        assert_eq!(index_kind(&FieldType::Relation { target: "A".into(), many: true }), Some(IndexKind::Keywords));
    }
}
