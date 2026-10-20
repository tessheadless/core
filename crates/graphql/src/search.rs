// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! `searchXxx` queries, the search settings of each content type (Admins)
//! and the `reindexSearch` mutation.

use std::sync::Arc;

use async_graphql::dynamic::{Field, FieldFuture, FieldValue, InputValue, TypeRef};
use serde_json::{Value, json};
use tess_core::query::{EntryQuery, Filter, Op};
use tess_core::{Cms, CmsError, ContentType};
use tess_search::{ModelSearchSettings, SearchIndex, SearchRequest, SearchService, indexable_fields};

use crate::SchemaConfig;
use crate::jsonobj::{json_object, object, plain, value};
use crate::naming::Names;
use crate::schema::{
    Connection, EntryNode, TypeDef, admin_mark_dirty, admin_require, cms_err, int_arg, opt_arg, require_read, stage_arg,
    stage_input,
};

pub(crate) fn search_query(ct: &Arc<ContentType>, n: &Names, config: &SchemaConfig) -> Field {
    let ct_ref = ct.clone();
    let max = config.max_page_size.min(250);
    Field::new(&n.search_query, TypeRef::named_nn(&n.connection), move |ctx| {
        let ct = ct_ref.clone();
        FieldFuture::new(async move {
            let stage = stage_arg(&ctx)?;
            require_read(&ctx, stage)?;
            let first = int_arg(&ctx, "first", 20)?;
            let skip = int_arg(&ctx, "skip", 0)?;
            if first < 0 || skip < 0 {
                return Err(cms_err(CmsError::validation("`first` and `skip` must be positive")));
            }
            let request = SearchRequest {
                text: ctx.args.try_get("query")?.string()?.to_string(),
                stage,
                limit: (first as usize).min(max),
                offset: skip as usize,
            };
            let hits = ctx.data::<SearchService>()?.search(&ct, &request).await.map_err(cms_err)?;
            // Load from the database: the index may lag behind or hold stale ids.
            let mut q = EntryQuery::new(stage);
            q.limit = hits.ids.len();
            q.filter = Some(Filter::cmp("id", Op::In, Value::Array(hits.ids.iter().map(|i| Value::String(i.to_string())).collect())));
            let mut found = if hits.ids.is_empty() {
                vec![]
            } else {
                ctx.data::<Cms>()?.query_entries(&ct.api_id, &q).await.map_err(cms_err)?.items
            };
            let items = hits
                .ids
                .iter()
                .filter_map(|id| {
                    let pos = found.iter().position(|e| e.id == *id)?;
                    let entry = found.swap_remove(pos);
                    let view = entry.view(stage)?;
                    Some(EntryNode { entry, stage, view })
                })
                .collect();
            Ok(Some(FieldValue::owned_any(Connection { items, total: hits.total })))
        })
    })
    .description(format!("Full-text search in {} entries, best matches first.", ct.api_id))
    .argument(InputValue::new("query", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("first", TypeRef::named(TypeRef::INT)).default_value(20))
    .argument(InputValue::new("skip", TypeRef::named(TypeRef::INT)).default_value(0))
    .argument(stage_input())
}

/// One content type's settings, with every field and how it would be indexed.
fn model_json(search: &SearchService, ct: &ContentType, settings: &ModelSearchSettings, indexed: Option<u64>) -> Value {
    let fields: Vec<Value> = indexable_fields(ct)
        .map(|(f, kind)| {
            let config = kind.and_then(|kind| {
                search.engine().field_config(&tess_search::IndexField { name: f.name.clone(), kind })
            });
            json!({
                "name": f.name,
                "type": crate::admin::kind_of(&f.field_type),
                "selected": settings.fields.contains(&f.name),
                "supported": kind.is_some(),
                "indexType": kind.map(|k| k.as_str()),
                "searchable": kind == Some(tess_search::IndexKind::Text),
                "engineConfig": config,
            })
        })
        .collect();
    json!({
        "apiId": ct.api_id,
        "enabled": settings.enabled,
        "active": SearchIndex::new(ct, settings).is_some(),
        "fields": fields,
        "indexedEntries": indexed,
    })
}

fn types() -> Vec<TypeDef> {
    let nn = TypeRef::named_nn;
    let field = json_object(
        "SearchFieldSettings",
        "A field of a content type, as the search engine would index it.",
        vec![
            plain("name", nn(TypeRef::STRING)),
            plain("type", nn(TypeRef::STRING)).doc("Field type, e.g. TEXT."),
            plain("selected", nn(TypeRef::BOOLEAN)).doc("Picked by an Admin for the index."),
            plain("supported", nn(TypeRef::BOOLEAN)).doc("False for JSON, components, dynamic zones and passwords."),
            plain("indexType", TypeRef::named(TypeRef::STRING))
                .doc("TEXT (full-text), KEYWORD, KEYWORDS, INT, FLOAT, BOOL or TIMESTAMP (Unix seconds)."),
            plain("searchable", nn(TypeRef::BOOLEAN)).doc("Matched by search queries (TEXT and MARKDOWN)."),
            plain("engineConfig", TypeRef::named("JSON")).doc("Field definition generated for the engine (Typesense)."),
        ],
    );
    let model = json_object(
        "SearchModelSettings",
        "Search settings of one content type.",
        vec![
            plain("apiId", nn(TypeRef::STRING)),
            plain("enabled", nn(TypeRef::BOOLEAN)),
            plain("active", nn(TypeRef::BOOLEAN))
                .doc("Enabled with at least one full-text field: the search<Plural> query exists."),
            object("fields", TypeRef::named_nn_list_nn("SearchFieldSettings")),
            plain("indexedEntries", TypeRef::named(TypeRef::INT)).doc("Entries indexed by an update; null otherwise."),
        ],
    );
    let settings = json_object(
        "SearchSettings",
        "Search engine and per-model settings.",
        vec![
            plain("engine", nn(TypeRef::STRING)).doc("typesense or memory."),
            object("models", TypeRef::named_nn_list_nn("SearchModelSettings")),
        ],
    );
    vec![TypeDef::Object(field), TypeDef::Object(model), TypeDef::Object(settings)]
}

/// Queries, mutations and types; the mutations exist only with an engine.
pub(crate) fn definitions(enabled: bool) -> (Vec<Field>, Vec<Field>, Vec<TypeDef>) {
    let settings = Field::new("searchSettings", TypeRef::named("SearchSettings"), |ctx| {
        FieldFuture::new(async move {
            admin_require(&ctx)?;
            let Ok(search) = ctx.data::<SearchService>() else { return Ok(None) };
            let cms = ctx.data::<Cms>()?;
            let all = search.all_settings().await.map_err(cms_err)?;
            let models: Vec<Value> = cms
                .content_types()
                .await
                .map_err(cms_err)?
                .iter()
                .filter(|ct| !crate::admin::BUILTIN_TYPES.contains(&ct.api_id.as_str()))
                .map(|ct| model_json(search, ct, &all.get(&ct.api_id).cloned().unwrap_or_default(), None))
                .collect();
            Ok(Some(value(json!({ "engine": search.engine().name(), "models": models }))))
        })
    })
    .description("Admins. Null when no search engine is configured (TESS_SEARCH).");
    if !enabled {
        return (vec![settings], vec![], types());
    }

    let update = Field::new("updateSearchSettings", TypeRef::named_nn("SearchModelSettings"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let search = ctx.data::<SearchService>()?;
            let api_id = ctx.args.try_get("apiId")?.string()?.to_string();
            let enabled = ctx.args.try_get("enabled")?.boolean()?;
            let fields: Vec<String> = ctx
                .args
                .try_get("fields")?
                .list()?
                .iter()
                .map(|v| v.string().map(String::from))
                .collect::<Result<_, _>>()?;
            let ct = ctx.data::<Cms>()?.content_type(&api_id).await.map_err(cms_err)?;
            let (saved, count) = search.update_settings(&ct, enabled, &fields, &actor).await.map_err(cms_err)?;
            admin_mark_dirty(&ctx)?;
            Ok(Some(value(model_json(search, &ct, &saved, Some(count)))))
        })
    })
    .description(
        "Admins. Enables or disables search on a content type and picks the indexed fields (at least one TEXT or \
         MARKDOWN field when enabled), then rebuilds its index from the database.",
    )
    .argument(InputValue::new("apiId", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("enabled", TypeRef::named_nn(TypeRef::BOOLEAN)))
    .argument(InputValue::new("fields", TypeRef::named_nn_list_nn(TypeRef::STRING)));

    let reindex = Field::new("reindexSearch", TypeRef::named_nn(TypeRef::INT), |ctx| {
        FieldFuture::new(async move {
            admin_require(&ctx)?;
            let cms = ctx.data::<Cms>()?;
            let search = ctx.data::<SearchService>()?;
            let only = opt_arg(&ctx, "apiId").map(|v| v.string().map(String::from)).transpose()?;
            let mut count = 0;
            for ct in cms.content_types().await.map_err(cms_err)? {
                if only.as_ref().is_none_or(|a| *a == ct.api_id) {
                    count += search.reindex(&ct).await.map_err(cms_err)?;
                }
            }
            Ok(Some(FieldValue::value(count as i64)))
        })
    })
    .description(
        "Rebuild search indexes from the database (every content type with search enabled, or one). Returns the \
         number of entries indexed.",
    )
    .argument(InputValue::new("apiId", TypeRef::named(TypeRef::STRING)));

    (vec![settings], vec![update, reindex], types())
}
