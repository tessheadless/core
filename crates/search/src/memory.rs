// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::collections::BTreeMap;

use async_trait::async_trait;
use tess_core::{ContentType, Result};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::{SearchDoc, SearchEngine, SearchHits, SearchRequest, text_fields};

/// Naive in-process engine (case-insensitive substring match on every word),
/// for tests and tiny installs.
#[derive(Default)]
pub struct MemoryEngine {
    indexes: RwLock<BTreeMap<String, BTreeMap<String, SearchDoc>>>,
}

#[async_trait]
impl SearchEngine for MemoryEngine {
    fn name(&self) -> &'static str {
        "memory"
    }

    async fn reset_index(&self, ct: &ContentType) -> Result<()> {
        self.indexes.write().await.insert(ct.api_id.clone(), BTreeMap::new());
        Ok(())
    }

    async fn drop_index(&self, content_type: &str) -> Result<()> {
        self.indexes.write().await.remove(content_type);
        Ok(())
    }

    async fn upsert(&self, ct: &ContentType, docs: Vec<SearchDoc>) -> Result<()> {
        let mut indexes = self.indexes.write().await;
        let index = indexes.entry(ct.api_id.clone()).or_default();
        for d in docs {
            index.insert(d.doc_id(), d);
        }
        Ok(())
    }

    async fn delete(&self, content_type: &str, doc_ids: Vec<String>) -> Result<()> {
        if let Some(index) = self.indexes.write().await.get_mut(content_type) {
            for id in doc_ids {
                index.remove(&id);
            }
        }
        Ok(())
    }

    async fn search(&self, ct: &ContentType, request: &SearchRequest) -> Result<SearchHits> {
        let indexes = self.indexes.read().await;
        let Some(index) = indexes.get(&ct.api_id) else {
            return Ok(SearchHits::default());
        };
        let words: Vec<String> = request.text.split_whitespace().map(str::to_lowercase).collect();
        let fields = text_fields(ct);
        let ids: Vec<_> = index
            .values()
            .filter(|d| d.stage == request.stage)
            .filter(|d| {
                let text = fields
                    .iter()
                    .filter_map(|f| d.fields.get(f).and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
                words.iter().all(|w| text.contains(w.as_str()))
            })
            .map(|d| d.entry_id)
            .collect();
        Ok(SearchHits {
            total: ids.len() as u64,
            ids: ids.into_iter().skip(request.offset).take(request.limit).collect(),
        })
    }
}
