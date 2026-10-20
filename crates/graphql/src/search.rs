// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! `searchXxx` queries and the `reindexSearch` mutation.

use std::sync::Arc;

use async_graphql::dynamic::{Field, FieldFuture, FieldValue, InputValue, TypeRef};
use tess_core::query::{EntryQuery, Filter, Op};
use tess_core::{Cms, CmsError, ContentType};
use tess_search::{SearchRequest, SearchService};
use serde_json::Value;

use crate::SchemaConfig;
use crate::naming::Names;
use crate::schema::{
    Connection, EntryNode, admin_require, cms_err, int_arg, opt_arg, require_read, stage_arg, stage_input,
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

pub(crate) fn reindex_mutation() -> Field {
    Field::new("reindexSearch", TypeRef::named_nn(TypeRef::INT), |ctx| {
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
    .description("Rebuild search indexes from the database (all content types, or one). Returns the number of entries indexed.")
    .argument(InputValue::new("apiId", TypeRef::named(TypeRef::STRING)))
}
