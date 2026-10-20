// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tess_core::{CmsError, Result, Stage};
use uuid::Uuid;

use crate::{
    IndexField, IndexKind, SearchDoc, SearchEngine, SearchHits, SearchIndex, SearchRequest,
};

#[derive(Debug, Clone)]
pub struct TypesenseConfig {
    /// e.g. `http://typesense:8108`
    pub url: String,
    /// Admin API key (from the environment / secrets manager).
    pub api_key: String,
    /// Prefix of collection names, to share a cluster between installs.
    pub collection_prefix: String,
}

/// [Typesense](https://typesense.org) engine: one collection per content type.
pub struct TypesenseEngine {
    http: Client,
    config: TypesenseConfig,
}

fn err(e: impl std::fmt::Display) -> CmsError {
    CmsError::Storage(format!("typesense: {e}"))
}

impl TypesenseEngine {
    pub fn new(config: TypesenseConfig) -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(err)?;
        Ok(TypesenseEngine { http, config })
    }

    fn collection(&self, content_type: &str) -> String {
        format!("{}{}", self.config.collection_prefix, content_type)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.config.url.trim_end_matches('/'))
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<(StatusCode, String)> {
        let res = req
            .header("X-TYPESENSE-API-KEY", &self.config.api_key)
            .send()
            .await
            .map_err(err)?;
        let status = res.status();
        Ok((status, res.text().await.map_err(err)?))
    }

    async fn ok(&self, req: reqwest::RequestBuilder) -> Result<String> {
        let (status, body) = self.send(req).await?;
        if status.is_success() {
            Ok(body)
        } else {
            Err(err(format!("HTTP {status}: {body}")))
        }
    }

    fn schema(&self, index: &SearchIndex) -> Value {
        let mut fields = vec![json!({"name": "stage", "type": "string", "facet": true})];
        fields.extend(index.fields.iter().map(field_schema));
        json!({"name": self.collection(&index.content_type), "fields": fields})
    }
}

/// Typesense field definition generated from the field type.
fn field_schema(f: &IndexField) -> Value {
    let (ty, facet, sort) = match f.kind {
        IndexKind::Text => ("string", false, false),
        IndexKind::Keyword => ("string", true, false),
        IndexKind::Keywords => ("string[]", true, false),
        IndexKind::Int | IndexKind::Timestamp => ("int64", false, true),
        IndexKind::Float => ("float", false, true),
        IndexKind::Bool => ("bool", true, false),
    };
    json!({"name": f.name, "type": ty, "facet": facet, "sort": sort, "optional": true})
}

#[async_trait]
impl SearchEngine for TypesenseEngine {
    fn name(&self) -> &'static str {
        "typesense"
    }

    fn field_config(&self, field: &IndexField) -> Option<Value> {
        Some(field_schema(field))
    }

    async fn reset_index(&self, index: &SearchIndex) -> Result<()> {
        self.drop_index(&index.content_type).await?;
        self.ok(self
            .http
            .post(self.url("/collections"))
            .json(&self.schema(index)))
            .await?;
        Ok(())
    }

    async fn drop_index(&self, content_type: &str) -> Result<()> {
        let path = format!("/collections/{}", self.collection(content_type));
        let (status, body) = self.send(self.http.delete(self.url(&path))).await?;
        if status.is_success() || status == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(err(format!("HTTP {status}: {body}")))
        }
    }

    async fn upsert(&self, index: &SearchIndex, docs: Vec<SearchDoc>) -> Result<()> {
        let mut jsonl = String::new();
        for d in &docs {
            let mut doc = d.fields.clone();
            doc.insert("id".into(), Value::String(d.doc_id()));
            doc.insert(
                "stage".into(),
                Value::String(
                    if d.stage == Stage::Draft {
                        "draft"
                    } else {
                        "published"
                    }
                    .into(),
                ),
            );
            jsonl.push_str(&Value::Object(doc).to_string());
            jsonl.push('\n');
        }
        let path = format!(
            "/collections/{}/documents/import?action=upsert",
            self.collection(&index.content_type)
        );
        let mut attempt = 0;
        loop {
            let (status, body) = self
                .send(self.http.post(self.url(&path)).body(jsonl.clone()))
                .await?;
            if status == StatusCode::NOT_FOUND && attempt == 0 {
                // Collection missing (fresh Typesense node): create it and retry.
                attempt += 1;
                self.ok(self
                    .http
                    .post(self.url("/collections"))
                    .json(&self.schema(index)))
                    .await?;
                continue;
            }
            if !status.is_success() {
                return Err(err(format!("HTTP {status}: {body}")));
            }
            // The import endpoint reports per-document results, one JSON per line.
            if let Some(failed) = body.lines().find(|l| !l.contains("\"success\":true")) {
                return Err(err(format!("import failed: {failed}")));
            }
            return Ok(());
        }
    }

    async fn delete(&self, content_type: &str, doc_ids: Vec<String>) -> Result<()> {
        for id in doc_ids {
            let path = format!(
                "/collections/{}/documents/{id}",
                self.collection(content_type)
            );
            let (status, body) = self.send(self.http.delete(self.url(&path))).await?;
            if !status.is_success() && status != StatusCode::NOT_FOUND {
                return Err(err(format!("HTTP {status}: {body}")));
            }
        }
        Ok(())
    }

    async fn search(&self, index: &SearchIndex, request: &SearchRequest) -> Result<SearchHits> {
        let stage = if request.stage == Stage::Draft {
            "draft"
        } else {
            "published"
        };
        let text = if request.text.trim().is_empty() {
            "*".to_string()
        } else {
            request.text.clone()
        };
        let limit = request.limit.clamp(1, 250).to_string();
        let offset = request.offset.to_string();
        let query_by = index.text_fields().join(",");
        let filter = format!("stage:={stage}");
        let path = format!(
            "/collections/{}/documents/search",
            self.collection(&index.content_type)
        );
        let req = self.http.get(self.url(&path)).query(&[
            ("q", text.as_str()),
            ("query_by", query_by.as_str()),
            ("filter_by", filter.as_str()),
            ("limit", limit.as_str()),
            ("offset", offset.as_str()),
            ("include_fields", "id"),
        ]);
        let (status, body) = self.send(req).await?;
        if status == StatusCode::NOT_FOUND {
            return Ok(SearchHits::default());
        }
        if !status.is_success() {
            return Err(err(format!("HTTP {status}: {body}")));
        }
        let v: Value = serde_json::from_str(&body).map_err(err)?;
        let ids = v["hits"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|h| h["document"]["id"].as_str())
            .filter_map(|id| {
                id.split_once('_')
                    .and_then(|(_, u)| Uuid::parse_str(u).ok())
            })
            .collect();
        let ids = if request.limit == 0 { vec![] } else { ids };
        Ok(SearchHits {
            ids,
            total: v["found"].as_u64().unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// Runs against `TESS_TEST_TYPESENSE_URL` / `TESS_TEST_TYPESENSE_KEY`; skipped when unset.
    #[tokio::test]
    async fn typesense_engine() {
        let Ok(url) = std::env::var("TESS_TEST_TYPESENSE_URL") else {
            eprintln!("TESS_TEST_TYPESENSE_URL not set, skipping");
            return;
        };
        let engine = TypesenseEngine::new(TypesenseConfig {
            url,
            api_key: std::env::var("TESS_TEST_TYPESENSE_KEY").expect("TESS_TEST_TYPESENSE_KEY"),
            collection_prefix: "tesstest_".into(),
        })
        .unwrap();
        crate::tests::scenario(Arc::new(engine)).await;
    }
}
