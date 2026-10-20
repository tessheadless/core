// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! GraphQL response cache (`TESS_CACHE`): anonymous read queries are served
//! from the cache; every change invalidates it (see `tess-cache`).

use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use tess_cache::{CacheService, Scope};
use tess_graphql::async_graphql::parser::{parse_query, types::OperationType};
use tess_graphql::async_graphql::Request;
use tess_graphql::{GraphqlService, Principal};

/// What a request does, from its selected operation(s).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Query,
    Mutation,
    /// Unparsable or subscription: executed as usual, never cached.
    Other,
}

pub(crate) fn kind(req: &Request) -> Kind {
    let Ok(doc) = parse_query(&req.query) else { return Kind::Other };
    let selected: Vec<OperationType> = doc
        .operations
        .iter()
        .filter(|(name, _)| match (&req.operation_name, name) {
            (Some(wanted), Some(name)) => name.as_str() == wanted,
            _ => true,
        })
        .map(|(_, op)| op.node.ty)
        .collect();
    if selected.contains(&OperationType::Mutation) {
        Kind::Mutation
    } else if !selected.is_empty() && selected.iter().all(|t| *t == OperationType::Query) {
        Kind::Query
    } else {
        Kind::Other
    }
}

fn json(body: Vec<u8>, cache: &'static str) -> Response {
    let mut res = body.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/graphql-response+json"));
    h.insert("x-tess-cache", HeaderValue::from_static(cache));
    res
}

/// Executes `req`, through the cache when it is an anonymous read query.
pub(crate) async fn execute(
    service: &GraphqlService,
    cache: Option<&CacheService>,
    req: Request,
    principal: Principal,
) -> Response {
    let Some(cache) = cache else {
        return async_graphql_axum::GraphQLResponse::from(service.execute(req, principal).await).into_response();
    };
    let kind = kind(&req);
    if kind != Kind::Query || !matches!(principal, Principal::Public) {
        let res = service.execute(req, principal).await;
        // Writes outside entries and models (settings, plugins...) may change
        // public responses too.
        if kind == Kind::Mutation && res.is_ok() {
            cache.invalidate(Scope::Content).await;
        }
        return async_graphql_axum::GraphQLResponse::from(res).into_response();
    }
    let variables = serde_json::to_vec(&req.variables).unwrap_or_default();
    let extensions = serde_json::to_vec(&req.extensions).unwrap_or_default();
    let operation = req.operation_name.clone().unwrap_or_default();
    let lookup = cache.lookup(Scope::Content, &[req.query.as_bytes(), operation.as_bytes(), &variables, &extensions]).await;
    if let Some(body) = lookup.value {
        return json(body, "HIT");
    }
    let res = service.execute(req, principal).await;
    let body = match serde_json::to_vec(&res) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "cannot serialize the response");
            return async_graphql_axum::GraphQLResponse::from(res).into_response();
        }
    };
    // Errors may be transient: never cached.
    let Some(key) = lookup.key else { return json(body, "BYPASS") }; // cache unavailable
    if res.is_ok() {
        cache.store(&key, &body).await;
    }
    json(body, "MISS")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(q: &str, op: Option<&str>) -> Request {
        let r = Request::new(q);
        match op {
            Some(op) => r.operation_name(op),
            None => r,
        }
    }

    #[test]
    fn request_kinds() {
        assert_eq!(kind(&req("{ posts { title } }", None)), Kind::Query);
        assert_eq!(kind(&req("query A { a } query B { b }", Some("B"))), Kind::Query);
        assert_eq!(kind(&req("mutation { deletePost(id: 1) }", None)), Kind::Mutation);
        assert_eq!(kind(&req("query A { a } mutation B { b }", Some("A"))), Kind::Query);
        assert_eq!(kind(&req("query A { a } mutation B { b }", None)), Kind::Mutation);
        assert_eq!(kind(&req("subscription { s }", None)), Kind::Other);
        assert_eq!(kind(&req("{ broken", None)), Kind::Other);
        assert_eq!(kind(&req("query A { a }", Some("Nope"))), Kind::Other);
    }
}
