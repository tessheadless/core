// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Drives the MCP tools over the real Streamable HTTP service, with the
//! caller's principal injected the way the server's auth middleware does.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use bytes::Bytes;
use http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tess_core::memory::MemoryStorage;
use tess_core::{Actor, Role};
use tess_graphql::ids::{IdCodec, IdConfig};
use tess_graphql::{GraphqlService, Principal, SchemaConfig};
use tess_mcp::http_service;
use tower::ServiceExt;

fn random_secret() -> Vec<u8> {
    let mut buf = vec![0u8; 32];
    getrandom::fill(&mut buf).unwrap();
    buf
}

fn service() -> Arc<GraphqlService> {
    let codec = IdCodec::new(IdConfig {
        secret: random_secret(),
        previous_secrets: vec![],
        ref_secret: random_secret(),
        ttl: Duration::from_secs(3600),
    })
    .unwrap();
    Arc::new(GraphqlService::new(Arc::new(MemoryStorage::new()), codec, SchemaConfig::default()))
}

fn actor(role: Role) -> Principal {
    Principal::User(Actor { id: None, name: format!("{role:?}"), role, scope: vec![] })
}

/// Sends one JSON-RPC message to `/mcp` as `principal` and returns the parsed body.
async fn rpc(svc: &Arc<GraphqlService>, principal: Principal, message: Value) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "localhost")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2025-06-18")
        .body(Body::from(message.to_string()))
        .unwrap();
    req.extensions_mut().insert(principal);
    let res = http_service(svc.clone(), vec!["localhost".into()]).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes: Bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or(Value::Null) };
    (status, body)
}

async fn call(svc: &Arc<GraphqlService>, principal: Principal, tool: &str, arguments: Value) -> Value {
    let (status, body) = rpc(
        svc,
        principal,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": tool, "arguments": arguments}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["result"].clone()
}

fn text(result: &Value) -> String {
    result["content"][0]["text"].as_str().unwrap_or_default().to_string()
}

const SETUP: &str = r#"mutation {
  createContentType(input: {apiId: "Post", fields: [
    {name: "title", type: TEXT, required: true}
  ]}) { apiId }
}"#;

async fn seed(svc: &Arc<GraphqlService>) {
    let req = async_graphql::Request::new(SETUP);
    let res = svc.execute(req, Principal::User(Actor::system())).await;
    assert!(res.errors.is_empty(), "{:?}", res.errors);
}

#[tokio::test]
async fn lists_the_two_tools_without_a_handshake() {
    let svc = service();
    let (status, body) = rpc(&svc, Principal::Public, json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let names: Vec<&str> = body["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["get_schema", "graphql_query"]);
}

#[tokio::test]
async fn get_schema_returns_the_schema_and_one_type() {
    let svc = service();
    seed(&svc).await;
    let all = call(&svc, Principal::Public, "get_schema", json!({})).await;
    assert!(text(&all).contains("type Post"), "{}", text(&all));

    let one = call(&svc, Principal::Public, "get_schema", json!({"type": "Post"})).await;
    assert!(text(&one).starts_with("type Post"), "{}", text(&one));
    assert!(!text(&one).contains("type Query"), "a single type must not include other types");
    assert_ne!(one["isError"], json!(true));

    let missing = call(&svc, Principal::Public, "get_schema", json!({"type": "Nope"})).await;
    assert_eq!(missing["isError"], json!(true));
    assert!(text(&missing).contains("Available types"));
}

#[tokio::test]
async fn anonymous_reads_published_content_only() {
    let svc = service();
    seed(&svc).await;
    let res = call(&svc, Principal::Public, "graphql_query", json!({"query": "{ posts { total } }"})).await;
    assert_ne!(res["isError"], json!(true), "{}", text(&res));
    let body: Value = serde_json::from_str(&text(&res)).unwrap();
    assert_eq!(body["data"]["posts"]["total"], 0);
}

#[tokio::test]
async fn anonymous_cannot_write() {
    let svc = service();
    seed(&svc).await;
    let res = call(
        &svc,
        Principal::Anonymous,
        "graphql_query",
        json!({"query": r#"mutation { createPost(data: {title: "x"}) { id } }"#}),
    )
    .await;
    assert_eq!(res["isError"], json!(true));
}

#[tokio::test]
async fn member_token_cannot_create_content() {
    let svc = service();
    seed(&svc).await;
    let res = call(
        &svc,
        actor(Role::Member),
        "graphql_query",
        json!({"query": r#"mutation { createPost(data: {title: "x"}) { id } }"#}),
    )
    .await;
    assert_eq!(res["isError"], json!(true), "{}", text(&res));
    assert!(text(&res).contains("FORBIDDEN"), "{}", text(&res));
}

#[tokio::test]
async fn contributor_can_create_then_read_a_draft() {
    let svc = service();
    seed(&svc).await;
    let created = call(
        &svc,
        actor(Role::Contributor),
        "graphql_query",
        json!({
            "query": "mutation($data: PostCreateInput!) { createPost(data: $data) { id stage } }",
            "variables": {"data": {"title": "Hello"}}
        }),
    )
    .await;
    assert_ne!(created["isError"], json!(true), "{}", text(&created));
    let body: Value = serde_json::from_str(&text(&created)).unwrap();
    assert_eq!(body["data"]["createPost"]["stage"], "DRAFT");
}

#[tokio::test]
async fn graphql_errors_are_reported_as_tool_errors() {
    let svc = service();
    seed(&svc).await;
    let res = call(&svc, Principal::Public, "graphql_query", json!({"query": "{ doesNotExist }"})).await;
    assert_eq!(res["isError"], json!(true));
    assert!(text(&res).contains("errors"));
}

#[tokio::test]
async fn depth_limit_is_enforced() {
    let svc = service();
    seed(&svc).await;
    // A balanced query nested deeper than the configured maximum (16).
    let depth = SchemaConfig::default().max_depth + 4;
    let mut query = String::from("{ posts { items { ");
    for _ in 0..depth {
        query.push_str("author { ");
    }
    query.push_str("id ");
    for _ in 0..depth {
        query.push_str("} ");
    }
    query.push_str("} } }");
    let res = call(&svc, Principal::Public, "graphql_query", json!({"query": query})).await;
    assert_eq!(res["isError"], json!(true), "{}", text(&res));
    assert!(text(&res).contains("Query is nested too deep."), "{}", text(&res));
}

#[tokio::test]
async fn unknown_tool_is_a_protocol_error() {
    let svc = service();
    let (_, body) = rpc(
        &svc,
        Principal::Public,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "drop_tables", "arguments": {}}}),
    )
    .await;
    assert!(body.get("error").is_some(), "{body}");
}

#[tokio::test]
async fn oversized_query_is_refused() {
    let svc = service();
    let query = format!("{{ posts(where: {{title_contains: \"{}\"}}) {{ total }} }}", "x".repeat(tess_mcp::MAX_QUERY_BYTES));
    let res = call(&svc, Principal::Public, "graphql_query", json!({"query": query})).await;
    assert_eq!(res["isError"], json!(true));
    assert!(text(&res).contains("limit"));
}

