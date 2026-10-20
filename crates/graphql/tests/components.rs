// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! New field types, components and dynamic zones through GraphQL.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tess_core::memory::MemoryStorage;
use tess_graphql::ids::{IdCodec, IdConfig};
use tess_graphql::{GraphqlService, Principal, SchemaConfig};

fn secret() -> Vec<u8> {
    let mut b = vec![0u8; 32];
    getrandom::fill(&mut b).unwrap();
    b
}

fn service() -> GraphqlService {
    let codec = IdCodec::new(IdConfig {
        secret: secret(),
        previous_secrets: vec![],
        ref_secret: secret(),
        ttl: Duration::from_secs(3600),
    })
    .unwrap();
    GraphqlService::new(Arc::new(MemoryStorage::new()), codec, SchemaConfig::default())
}

fn admin() -> Principal {
    Principal::User(tess_core::Actor::system())
}

async fn run(svc: &GraphqlService, who: &Principal, query: &str, vars: Value) -> Value {
    let req = async_graphql::Request::new(query).variables(async_graphql::Variables::from_json(vars));
    serde_json::to_value(svc.execute(req, who.clone()).await).unwrap()
}

async fn ok(svc: &GraphqlService, who: &Principal, query: &str, vars: Value) -> Value {
    let res = run(svc, who, query, vars).await;
    assert!(res.get("errors").is_none(), "unexpected errors: {res:#}");
    res["data"].clone()
}

async fn error(svc: &GraphqlService, query: &str, vars: Value) -> (String, String) {
    let res = run(svc, &admin(), query, vars).await;
    let e = &res["errors"][0];
    (
        e["extensions"]["code"].as_str().unwrap_or_default().to_string(),
        e["message"].as_str().unwrap_or_default().to_string(),
    )
}

#[tokio::test]
async fn components_dynamic_zones_and_new_types() {
    let svc = service();
    let a = admin();

    // STRING is gone; TEXT defaults to 80 characters.
    let (_, msg) = error(&svc, r#"mutation { createContentType(input: {apiId: "X", fields: [{name: "a", type: STRING}]}) { apiId } }"#, json!({})).await;
    assert!(msg.contains("STRING"), "{msg}");

    ok(&svc, &a, r#"mutation { createContentType(input: {apiId: "Author", fields: [{name: "name", type: TEXT, required: true}]}) { apiId } }"#, json!({})).await;
    ok(
        &svc,
        &a,
        r#"mutation {
            l: createComponent(input: {apiId: "Link", fields: [
                {name: "label", type: TEXT, required: true, maxLength: 20},
                {name: "url", type: TEXT, maxLength: null, pattern: "https://\\S+"},
                {name: "author", type: RELATION, relationTarget: "Author"}
            ]}) { apiId fields { name type maxLength pattern } }
            h: createComponent(input: {apiId: "Hero", description: "Big banner", fields: [
                {name: "title", type: TEXT},
                {name: "kind", type: ENUM, enumValues: ["WIDE", "NARROW"]},
                {name: "cta", type: COMPONENT, component: "Link"}
            ]}) { apiId }
        }"#,
        json!({}),
    )
    .await;
    let d = ok(
        &svc,
        &a,
        r#"mutation { createContentType(input: {apiId: "Page", fields: [
            {name: "title", type: TEXT, required: true},
            {name: "email", type: TEXT, pattern: "[^@\\s]+@[^@\\s]+\\.[a-z]{2,}"},
            {name: "body", type: MARKDOWN},
            {name: "day", type: DATE},
            {name: "code", type: UUID},
            {name: "secret", type: PASSWORD, minLength: 10},
            {name: "links", type: COMPONENT, component: "Link", repeatable: true},
            {name: "blocks", type: DYNAMIC_ZONE, components: ["Hero", "Link"]}
        ]}) { fields { name type minLength maxLength component repeatable components } } }"#,
        json!({}),
    )
    .await;
    let fields = &d["createContentType"]["fields"];
    assert_eq!(fields[0]["maxLength"], 80, "TEXT defaults to 80");
    assert_eq!(fields[5]["minLength"], 10);
    assert_eq!(fields[7]["components"], json!(["Hero", "Link"]));

    let author = ok(&svc, &a, r#"mutation { createAuthor(data: {name: "Ada Example"}) { id } }"#, json!({})).await;
    let author_id = author["createAuthor"]["id"].clone();
    let create = r#"mutation($d: PageCreateInput!) { createPage(data: $d) {
        id title body day code
        links { label url author { name } }
        blocks {
            __typename
            ... on Hero { title kind cta { label url author { name } } }
            ... on Link { label }
        }
    } }"#;
    let d = ok(
        &svc,
        &a,
        create,
        json!({"d": {
            "title": "Home", "email": "ada@example.com", "body": "# Hi\n\n<Callout>MDX ok</Callout>", "day": "2026-10-18",
            "secret": "a long secret",
            "links": [{"label": "Docs", "url": "https://tessheadless.com/docs/", "author": author_id}],
            "blocks": [
                {"Hero": {"title": "Welcome", "kind": "WIDE", "cta": {"label": "Start", "url": "https://example.com", "author": author_id}}},
                {"Link": {"label": "More"}}
            ]
        }}),
    )
    .await;
    let page = &d["createPage"];
    assert_eq!(page["links"][0]["author"]["name"], "Ada Example");
    assert_eq!(page["blocks"][0]["__typename"], "Hero");
    assert_eq!(page["blocks"][0]["cta"]["author"]["name"], "Ada Example");
    assert_eq!(page["blocks"][1], json!({"__typename": "Link", "label": "More"}));
    assert_eq!(page["code"].as_str().unwrap().len(), 36, "UUID generated");
    let id = page["id"].clone();

    // Passwords: write-only, verifiable.
    let (code, _) = error(&svc, "query($id: ID!) { page(id: $id, stage: DRAFT) { secret } }", json!({"id": id})).await;
    assert_eq!(code, "", "unknown field `secret` is a validation error");
    let v = ok(
        &svc,
        &a,
        r#"query($id: ID!) { yes: verifyPassword(contentType: "Page", id: $id, field: "secret", password: "a long secret", stage: DRAFT)
                            no: verifyPassword(contentType: "Page", id: $id, field: "secret", password: "nope", stage: DRAFT) }"#,
        json!({"id": id}),
    )
    .await;
    assert_eq!(v, json!({"yes": true, "no": false}));

    // Validation errors with paths.
    for (data, needle) in [
        (json!({"title": "x", "body": "<div>no html</div>"}), "contains HTML"),
        (json!({"title": "x", "email": "nope"}), "format"),
        (json!({"title": "x".repeat(81)}), "limited to 80"),
        (json!({"title": "x", "day": "18/10/2026"}), "a date"),
        (json!({"title": "x", "secret": "short"}), "at least 10"),
        (json!({"title": "x", "links": [{"label": "x", "url": "ftp://x"}]}), "links[0].url"),
        (json!({"title": "x", "blocks": [{"Hero": {"title": "a"}, "Link": {"label": "b"}}]}), "exactly one"),
    ] {
        let (code, msg) = error(&svc, "mutation($d: PageCreateInput!) { createPage(data: $d) { id } }", json!({"d": data})).await;
        assert_eq!(code, "BAD_USER_INPUT", "{msg}");
        assert!(msg.contains(needle), "{needle}: {msg}");
    }

    // Filters on the new types.
    let f = ok(&svc, &a, r#"{ a: pages(stage: DRAFT, where: {day_gte: "2026-01-01"}) { total } b: pages(stage: DRAFT, where: {title_contains: "Hom"}) { total } }"#, json!({})).await;
    assert_eq!(f, json!({"a": {"total": 1}, "b": {"total": 1}}));

    // Component management.
    let (code, msg) = error(&svc, r#"mutation { deleteComponent(apiId: "Link") }"#, json!({})).await;
    assert_eq!(code, "CONFLICT", "{msg}");
    let (code, _) = error(&svc, r#"mutation { createComponent(input: {apiId: "Page", fields: [{name: "a", type: TEXT}]}) { apiId } }"#, json!({})).await;
    assert_eq!(code, "CONFLICT");
    let list = ok(&svc, &a, "{ components { apiId description fields { name type } } }", json!({})).await;
    assert_eq!(list["components"].as_array().unwrap().len(), 2);
    // Adding a field to a component is immediately available everywhere.
    ok(
        &svc,
        &a,
        r#"mutation { updateComponent(apiId: "Hero", input: {apiId: "Hero", fields: [
            {name: "title", type: TEXT}, {name: "kind", type: ENUM, enumValues: ["WIDE", "NARROW"]},
            {name: "cta", type: COMPONENT, component: "Link"}, {name: "subtitle", type: TEXT}
        ]}) { apiId } }"#,
        json!({}),
    )
    .await;
    let d = ok(&svc, &a, "{ pages(stage: DRAFT) { items { blocks { ... on Hero { subtitle } } } } }", json!({})).await;
    assert_eq!(d["pages"]["items"][0]["blocks"][0]["subtitle"], Value::Null);
}
