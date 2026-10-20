// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! MongoDB backend. Each entry is one document whose `draft` / `published`
//! sub-documents hold content fields plus system fields (see
//! [`Entry::view`]), so filters and sorting are pushed down to MongoDB.

use async_trait::async_trait;
use futures::TryStreamExt;
use mongodb::bson::{self, Bson, Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Client, Collection, Database, IndexModel};
use serde_json::{Map, Value};
use tess_core::model::format_ts;
use tess_core::query::{Direction, Filter, Op};
use tess_core::{CmsError, ContentType, Entry, EntryQuery, Page, Result, Stage, Storage};
use uuid::Uuid;

pub struct MongoStorage {
    db: Database,
}

impl MongoStorage {
    pub async fn connect(url: &str, database: &str) -> Result<Self> {
        let client = Client::with_uri_str(url).await.map_err(CmsError::storage)?;
        Ok(MongoStorage {
            db: client.database(database),
        })
    }

    fn content_types(&self) -> Collection<Document> {
        self.db.collection("tess_content_types")
    }

    fn entries(&self) -> Collection<Document> {
        self.db.collection("tess_entries")
    }

    fn settings(&self) -> Collection<Document> {
        self.db.collection("tess_settings")
    }
}

fn stage_key(stage: Stage) -> &'static str {
    match stage {
        Stage::Draft => "draft",
        Stage::Published => "published",
    }
}

fn to_bson(value: &Value) -> Result<Bson> {
    bson::to_bson(value).map_err(CmsError::storage)
}

fn to_json(value: Bson) -> Value {
    value.into_relaxed_extjson()
}

fn entry_to_doc(entry: &Entry) -> Result<Document> {
    let view = |stage| -> Result<Bson> {
        match entry.view(stage) {
            Some(v) => to_bson(&Value::Object(v)),
            None => Ok(Bson::Null),
        }
    };
    Ok(doc! {
        "_id": entry.id.to_string(),
        "contentType": &entry.content_type,
        "createdAt": format_ts(entry.created_at),
        "updatedAt": format_ts(entry.updated_at),
        "publishedAt": entry.published_at.map(format_ts),
        "draft": view(Stage::Draft)?,
        "published": view(Stage::Published)?,
    })
}

fn doc_to_entry(mut d: Document) -> Result<Entry> {
    let bad = |what: &str| CmsError::Storage(format!("malformed entry document: {what}"));
    let mut view = |k: &str| -> Option<Map<String, Value>> {
        match d.remove(k).map(to_json) {
            Some(Value::Object(m)) => Some(m),
            _ => None,
        }
    };
    let draft = view("draft").ok_or_else(|| bad("draft"))?;
    let published = view("published");
    let id = d
        .get_str("_id")
        .ok()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| bad("_id"))?;
    let content_type = d
        .get_str("contentType")
        .map_err(|_| bad("contentType"))?
        .to_string();
    Entry::from_views(content_type, id, draft, published).map_err(CmsError::Storage)
}

/// Escapes a literal for use inside a MongoDB (PCRE) regex.
fn regex_literal(s: &str) -> String {
    regex::escape(s)
}

fn translate(filter: &Filter, stage: &str) -> Result<Document> {
    Ok(match filter {
        // An empty $and/$or is rejected by MongoDB.
        Filter::And(fs) if fs.is_empty() => doc! {},
        Filter::Or(fs) if fs.is_empty() => doc! { "_id": { "$exists": false } },
        Filter::And(fs) => {
            doc! { "$and": fs.iter().map(|f| translate(f, stage)).collect::<Result<Vec<_>>>()? }
        }
        Filter::Or(fs) => {
            doc! { "$or": fs.iter().map(|f| translate(f, stage)).collect::<Result<Vec<_>>>()? }
        }
        Filter::Not(f) => doc! { "$nor": [translate(f, stage)?] },
        Filter::Cmp { field, op, value } => {
            let path = format!("{stage}.{field}");
            let v = to_bson(value)?;
            let cond = match op {
                Op::Eq => return Ok(doc! { path: v }),
                Op::Ne => doc! { "$ne": v },
                Op::Lt => doc! { "$lt": v },
                Op::Lte => doc! { "$lte": v },
                Op::Gt => doc! { "$gt": v },
                Op::Gte => doc! { "$gte": v },
                Op::In | Op::NotIn if !value.is_array() => {
                    let none = doc! { "_id": { "$exists": false } };
                    return Ok(if *op == Op::In {
                        none
                    } else {
                        doc! { "$nor": [none] }
                    });
                }
                Op::In => doc! { "$in": v },
                Op::NotIn => doc! { "$nin": v },
                Op::Contains | Op::StartsWith | Op::EndsWith => {
                    let Value::String(s) = value else {
                        return Ok(doc! { "_id": { "$exists": false } });
                    };
                    let lit = regex_literal(s);
                    // \A / \z anchor at the true string ends, unlike ^ / $
                    // which also match around newlines in multiline mode.
                    let pattern = match op {
                        Op::Contains => lit,
                        Op::StartsWith => format!("\\A{lit}"),
                        _ => format!("{lit}\\z"),
                    };
                    doc! { "$regex": pattern }
                }
            };
            doc! { path: cond }
        }
    })
}

fn base_filter(content_type: &str, query: &EntryQuery) -> Result<Document> {
    let stage = stage_key(query.stage);
    let mut clauses = vec![
        doc! { "contentType": content_type },
        doc! { stage: { "$type": "object" } },
    ];
    if let Some(f) = &query.filter {
        clauses.push(translate(f, stage)?);
    }
    Ok(doc! { "$and": clauses })
}

#[async_trait]
impl Storage for MongoStorage {
    fn name(&self) -> &'static str {
        "mongodb"
    }

    async fn migrate(&self) -> Result<()> {
        let index = IndexModel::builder()
            .keys(doc! { "contentType": 1, "draft.createdAt": 1 })
            .options(
                IndexOptions::builder()
                    .name("content_type_created".to_string())
                    .build(),
            )
            .build();
        self.entries()
            .create_index(index)
            .await
            .map_err(CmsError::storage)?;
        Ok(())
    }

    async fn list_content_types(&self) -> Result<Vec<ContentType>> {
        let docs: Vec<Document> = self
            .content_types()
            .find(doc! {})
            .sort(doc! { "_id": 1 })
            .await
            .map_err(CmsError::storage)?
            .try_collect()
            .await
            .map_err(CmsError::storage)?;
        docs.into_iter().map(doc_to_content_type).collect()
    }

    async fn get_content_type(&self, api_id: &str) -> Result<Option<ContentType>> {
        let doc = self
            .content_types()
            .find_one(doc! { "_id": api_id })
            .await
            .map_err(CmsError::storage)?;
        doc.map(doc_to_content_type).transpose()
    }

    async fn put_content_type(&self, content_type: &ContentType) -> Result<()> {
        let definition = serde_json::to_string(content_type).map_err(CmsError::storage)?;
        self.content_types()
            .replace_one(
                doc! { "_id": &content_type.api_id },
                doc! { "_id": &content_type.api_id, "definition": definition },
            )
            .upsert(true)
            .await
            .map_err(CmsError::storage)?;
        Ok(())
    }

    async fn delete_content_type(&self, api_id: &str) -> Result<()> {
        self.entries()
            .delete_many(doc! { "contentType": api_id })
            .await
            .map_err(CmsError::storage)?;
        self.content_types()
            .delete_one(doc! { "_id": api_id })
            .await
            .map_err(CmsError::storage)?;
        Ok(())
    }

    async fn get_entry(&self, content_type: &str, id: Uuid) -> Result<Option<Entry>> {
        let doc = self
            .entries()
            .find_one(doc! { "_id": id.to_string(), "contentType": content_type })
            .await
            .map_err(CmsError::storage)?;
        doc.map(doc_to_entry).transpose()
    }

    async fn put_entry(&self, entry: &Entry) -> Result<()> {
        self.entries()
            .replace_one(doc! { "_id": entry.id.to_string() }, entry_to_doc(entry)?)
            .upsert(true)
            .await
            .map_err(CmsError::storage)?;
        Ok(())
    }

    async fn delete_entry(&self, content_type: &str, id: Uuid) -> Result<bool> {
        let res = self
            .entries()
            .delete_one(doc! { "_id": id.to_string(), "contentType": content_type })
            .await
            .map_err(CmsError::storage)?;
        Ok(res.deleted_count > 0)
    }

    async fn query_entries(&self, content_type: &str, query: &EntryQuery) -> Result<Page> {
        let filter = base_filter(content_type, query)?;
        let stage = stage_key(query.stage);
        let mut sort = Document::new();
        for key in query.effective_sort() {
            let dir = if key.direction == Direction::Asc {
                1
            } else {
                -1
            };
            sort.insert(format!("{stage}.{}", key.field), dir);
        }
        let total = self
            .entries()
            .count_documents(filter.clone())
            .await
            .map_err(CmsError::storage)?;
        let docs: Vec<Document> = self
            .entries()
            .find(filter)
            .sort(sort)
            .skip(query.offset as u64)
            .limit(query.limit as i64)
            .await
            .map_err(CmsError::storage)?
            .try_collect()
            .await
            .map_err(CmsError::storage)?;
        let items = docs
            .into_iter()
            .map(doc_to_entry)
            .collect::<Result<Vec<_>>>()?;
        Ok(Page { items, total })
    }

    // Settings are stored as JSON strings: values are opaque to queries and
    // this preserves them exactly.
    async fn get_setting(&self, key: &str) -> Result<Option<Value>> {
        let doc = self
            .settings()
            .find_one(doc! { "_id": key })
            .await
            .map_err(CmsError::storage)?;
        doc.map(|d| {
            serde_json::from_str(d.get_str("value").map_err(CmsError::storage)?)
                .map_err(CmsError::storage)
        })
        .transpose()
    }

    async fn put_setting(&self, key: &str, value: &Value) -> Result<()> {
        let value = serde_json::to_string(value).map_err(CmsError::storage)?;
        self.settings()
            .replace_one(doc! { "_id": key }, doc! { "_id": key, "value": value })
            .upsert(true)
            .await
            .map_err(CmsError::storage)?;
        Ok(())
    }

    async fn delete_setting(&self, key: &str) -> Result<bool> {
        let res = self
            .settings()
            .delete_one(doc! { "_id": key })
            .await
            .map_err(CmsError::storage)?;
        Ok(res.deleted_count > 0)
    }

    async fn list_settings(&self, prefix: &str) -> Result<Vec<(String, Value)>> {
        let filter = doc! { "_id": { "$regex": format!("\\A{}", regex_literal(prefix)) } };
        let docs: Vec<Document> = self
            .settings()
            .find(filter)
            .sort(doc! { "_id": 1 })
            .await
            .map_err(CmsError::storage)?
            .try_collect()
            .await
            .map_err(CmsError::storage)?;
        docs.into_iter()
            .map(|d| {
                let key = d.get_str("_id").map_err(CmsError::storage)?.to_string();
                let value = serde_json::from_str(d.get_str("value").map_err(CmsError::storage)?)
                    .map_err(CmsError::storage)?;
                Ok((key, value))
            })
            .collect()
    }
}

fn doc_to_content_type(d: Document) -> Result<ContentType> {
    let def = d.get_str("definition").map_err(CmsError::storage)?;
    serde_json::from_str(def).map_err(CmsError::storage)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// Runs against `TESS_TEST_MONGODB_URL`; skipped when unset.
    #[tokio::test]
    async fn conformance() {
        let Ok(url) = std::env::var("TESS_TEST_MONGODB_URL") else {
            eprintln!("TESS_TEST_MONGODB_URL not set, skipping");
            return;
        };
        let storage = MongoStorage::connect(&url, "tess_conformance")
            .await
            .unwrap();
        tess_core::testkit::run_conformance(Arc::new(storage)).await;
    }
}
