// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Full-text search. Entries are pushed to a [`SearchEngine`] by
//! [`SearchService`], a built-in hook, whenever they change; queries return
//! entry ids that are then loaded from the primary database, so search results
//! always reflect stored content and permissions.
//!
//! Implementations: [`TypesenseEngine`] (Typesense), and [`MemoryEngine`] for
//! tests. Other engines (Meilisearch, OpenSearch, ...) implement the trait.

mod memory;
mod typesense;

use std::sync::Arc;

use async_trait::async_trait;
use tess_core::query::EntryQuery;
use tess_core::{Cms, ContentType, Entry, EntryAction, EntryEvent, FieldType, Hook, Result, Stage, Storage};
use serde_json::{Map, Value};
use uuid::Uuid;

pub use memory::MemoryEngine;
pub use typesense::{TypesenseConfig, TypesenseEngine};

/// A document as sent to the engine: one per (entry, stage).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchDoc {
    pub entry_id: Uuid,
    pub stage: Stage,
    /// Indexable fields only (see [`indexable`]).
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
    /// (Re)creates the index of a content type, dropping existing documents.
    async fn reset_index(&self, ct: &ContentType) -> Result<()>;
    async fn drop_index(&self, content_type: &str) -> Result<()>;
    async fn upsert(&self, ct: &ContentType, docs: Vec<SearchDoc>) -> Result<()>;
    async fn delete(&self, content_type: &str, doc_ids: Vec<String>) -> Result<()>;
    async fn search(&self, ct: &ContentType, request: &SearchRequest) -> Result<SearchHits>;
}

/// Fields sent to the search engine. Relations, JSON, date-times, UUIDs,
/// passwords and components are left out.
pub fn indexable(ct: &ContentType) -> impl Iterator<Item = &tess_core::FieldDef> {
    ct.fields.iter().filter(|f| {
        matches!(
            f.field_type,
            FieldType::Text { .. }
                | FieldType::Markdown
                | FieldType::Date
                | FieldType::Int
                | FieldType::Float
                | FieldType::Boolean
                | FieldType::Enum { .. }
        )
    })
}

/// Text fields searched by queries. A content type without any is not searchable.
pub fn text_fields(ct: &ContentType) -> Vec<String> {
    ct.fields.iter().filter(|f| f.field_type.is_textual()).map(|f| f.name.clone()).collect()
}

pub fn is_searchable(ct: &ContentType) -> bool {
    !text_fields(ct).is_empty()
}

fn docs_for(ct: &ContentType, entry: &Entry) -> Vec<SearchDoc> {
    [Stage::Draft, Stage::Published]
        .into_iter()
        .filter_map(|stage| {
            let data = entry.data(stage)?;
            let fields = indexable(ct)
                .filter_map(|f| {
                    let v = data.get(&f.name)?;
                    let ok = match f.field_type {
                        FieldType::Int => v.is_i64(),
                        FieldType::Float => v.is_number(),
                        FieldType::Boolean => v.is_boolean(),
                        _ => v.is_string(),
                    };
                    ok.then(|| (f.name.clone(), v.clone()))
                })
                .collect();
            Some(SearchDoc { entry_id: entry.id, stage, fields })
        })
        .collect()
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

    /// Rebuilds the index of one content type from the database.
    pub async fn reindex(&self, ct: &ContentType) -> Result<u64> {
        self.engine.reset_index(ct).await?;
        if !is_searchable(ct) {
            return Ok(0);
        }
        let mut count = 0;
        let mut q = EntryQuery::new(Stage::Draft);
        q.limit = 500;
        loop {
            let page = self.storage.query_entries(&ct.api_id, &q).await?;
            let docs: Vec<_> = page.items.iter().flat_map(|e| docs_for(ct, e)).collect();
            count += page.items.len() as u64;
            if !docs.is_empty() {
                self.engine.upsert(ct, docs).await?;
            }
            q.offset += page.items.len();
            if page.items.is_empty() || q.offset as u64 >= page.total {
                return Ok(count);
            }
        }
    }

    /// Ids of matching entries, best first.
    pub async fn search(&self, ct: &ContentType, request: &SearchRequest) -> Result<SearchHits> {
        if !is_searchable(ct) {
            return Ok(SearchHits::default());
        }
        self.engine.search(ct, request).await
    }

    async fn sync_entry(&self, event: &EntryEvent) -> Result<()> {
        let Some(ct) = self.storage.get_content_type(&event.content_type).await? else {
            return Ok(());
        };
        if !is_searchable(&ct) {
            return Ok(());
        }
        let id = event.entry.as_ref().or(event.previous.as_ref()).map(|e| e.id).unwrap_or_default();
        let docs = event.entry.as_ref().map(|e| docs_for(&ct, e)).unwrap_or_default();
        // Stages that no longer exist (unpublish, delete) are removed.
        let stale: Vec<String> = [Stage::Draft, Stage::Published]
            .into_iter()
            .filter(|s| !docs.iter().any(|d| d.stage == *s))
            .map(|s| doc_id(id, s))
            .collect();
        if !docs.is_empty() {
            self.engine.upsert(&ct, docs).await?;
        }
        if !stale.is_empty() && event.action != EntryAction::Create {
            self.engine.delete(&ct.api_id, stale).await?;
        }
        Ok(())
    }
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
        let res = if deleted { self.0.engine.drop_index(&ct.api_id).await } else { self.0.reindex(ct).await.map(|_| ()) };
        if let Err(e) = res {
            tracing::warn!(content_type = %ct.api_id, error = %e, "search index update failed");
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use serde_json::json;

    use super::*;
    use tess_core::memory::MemoryStorage;
    use tess_core::{ContentTypeDef, FieldDef};

    pub(crate) fn data(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    /// Shared scenario run against every engine.
    pub(crate) async fn scenario(engine: Arc<dyn SearchEngine>) {
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let cms = Cms::new(storage.clone());
        let search = SearchService::new(engine, storage);
        search.install(&cms);
        let f = |name: &str, field_type| FieldDef { name: name.into(), field_type, required: false, description: None };
        let ct = cms
            .create_content_type(ContentTypeDef { workflow: false,
                api_id: "SearchTkArticle".into(),
                plural_api_id: None,
                description: None,
                fields: vec![f("title", FieldType::text()), f("body", FieldType::text()), f("views", FieldType::Int), f("meta", FieldType::Json)],
            }, &tess_core::Actor::system())
            .await
            .unwrap();
        let q = |text: &str, stage| SearchRequest { text: text.into(), stage, limit: 10, offset: 0 };

        let rust = cms.create_entry(&ct.api_id, data(json!({"title": "Rust ownership", "body": "Borrowing explained", "views": 3, "meta": {"x": 1}})), &tess_core::Actor::system()).await.unwrap();
        let cms_doc = cms.create_entry(&ct.api_id, data(json!({"title": "Headless CMS", "body": "Content with GraphQL"})), &tess_core::Actor::system()).await.unwrap();
        cms.publish_entry(&ct.api_id, rust.id, &tess_core::Actor::system()).await.unwrap();

        let hits = search.search(&ct, &q("borrowing", Stage::Draft)).await.unwrap();
        assert_eq!(hits.ids, vec![rust.id]);
        assert_eq!(search.search(&ct, &q("graphql", Stage::Published)).await.unwrap().total, 0, "drafts are not public");
        assert_eq!(search.search(&ct, &q("graphql", Stage::Draft)).await.unwrap().ids, vec![cms_doc.id]);

        // Updates replace the indexed draft but not the published version.
        cms.update_entry(&ct.api_id, rust.id, data(json!({"title": "Rust lifetimes"})), &tess_core::Actor::system()).await.unwrap();
        assert_eq!(search.search(&ct, &q("lifetimes", Stage::Draft)).await.unwrap().total, 1);
        assert_eq!(search.search(&ct, &q("lifetimes", Stage::Published)).await.unwrap().total, 0);
        assert_eq!(search.search(&ct, &q("ownership", Stage::Published)).await.unwrap().total, 1);

        cms.unpublish_entry(&ct.api_id, rust.id, &tess_core::Actor::system()).await.unwrap();
        assert_eq!(search.search(&ct, &q("ownership", Stage::Published)).await.unwrap().total, 0);
        cms.delete_entry(&ct.api_id, rust.id, &tess_core::Actor::system()).await.unwrap();
        assert_eq!(search.search(&ct, &q("lifetimes", Stage::Draft)).await.unwrap().total, 0);

        // Reindex from the database.
        search.engine().reset_index(&ct).await.unwrap();
        assert_eq!(search.search(&ct, &q("graphql", Stage::Draft)).await.unwrap().total, 0);
        assert_eq!(search.reindex(&ct).await.unwrap(), 1);
        assert_eq!(search.search(&ct, &q("graphql", Stage::Draft)).await.unwrap().total, 1);

        cms.delete_content_type(&ct.api_id, &tess_core::Actor::system()).await.unwrap();
    }

    #[tokio::test]
    async fn memory_engine() {
        scenario(Arc::new(MemoryEngine::default())).await;
    }
}
