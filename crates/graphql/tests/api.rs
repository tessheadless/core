// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! End-to-end tests of the generated GraphQL API on the in-memory backend.

use std::sync::Arc;
use std::time::Duration;

use tess_core::memory::MemoryStorage;
use tess_graphql::ids::{IdCodec, IdConfig};
use tess_graphql::{GraphqlService, Principal, SchemaConfig};
use serde_json::{Value, json};

fn random_secret() -> Vec<u8> {
    let mut buf = vec![0u8; 32];
    getrandom::fill(&mut buf).unwrap();
    buf
}

fn service() -> GraphqlService {
    let codec = IdCodec::new(IdConfig {
        secret: random_secret(),
        previous_secrets: vec![],
        ref_secret: random_secret(),
        ttl: Duration::from_secs(3600),
    })
    .unwrap();
    GraphqlService::new(Arc::new(MemoryStorage::new()), codec, SchemaConfig::default())
}

async fn run(svc: &GraphqlService, who: &Principal, query: &str, vars: Value) -> Value {
    let req = async_graphql::Request::new(query).variables(async_graphql::Variables::from_json(vars));
    let res = svc.execute(req, who.clone()).await;
    serde_json::to_value(&res).unwrap()
}

async fn ok(svc: &GraphqlService, who: &Principal, query: &str, vars: Value) -> Value {
    let res = run(svc, who, query, vars).await;
    assert!(res.get("errors").is_none(), "unexpected errors: {res:#}");
    res["data"].clone()
}

fn sys() -> Principal {
    Principal::User(tess_core::Actor::system())
}

fn error_code(res: &Value) -> &str {
    res["errors"][0]["extensions"]["code"].as_str().unwrap_or_default()
}

const SETUP: &str = r#"
mutation {
  a: createContentType(input: {apiId: "Author", fields: [{name: "name", type: TEXT, required: true}]}) { apiId }
  p: createContentType(input: {apiId: "Post", fields: [
    {name: "title", type: TEXT, required: true},
    {name: "views", type: INT},
    {name: "publishedOn", type: DATETIME},
    {name: "status", type: ENUM, enumValues: ["DRAFTED", "LIVE"]},
    {name: "extra", type: JSON},
    {name: "author", type: RELATION, relationTarget: "Author"},
    {name: "coAuthors", type: RELATION, relationTarget: "Author", many: true}
  ]}) { apiId pluralApiId fields { name type required } }
}"#;

#[tokio::test]
async fn content_lifecycle() {
    let svc = service();
    let admin = sys();
    let created = ok(&svc, &admin, SETUP, json!({})).await;
    assert_eq!(created["p"]["pluralApiId"], "posts");

    let author = ok(&svc, &admin, r#"mutation { createAuthor(data: {name: "Writer A"}) { id ref } }"#, json!({})).await;
    let author_id = author["createAuthor"]["id"].as_str().unwrap().to_string();
    ok(&svc, &admin, "mutation($id: ID!) { publishAuthor(id: $id) { id } }", json!({"id": author_id})).await;

    let mut post_ids = Vec::new();
    for (title, views) in [("Hello", 5), ("World", 50), ("Draft only", 1)] {
        let res = ok(
            &svc,
            &admin,
            r#"mutation($data: PostCreateInput!) { createPost(data: $data) { id title stage author { name } } }"#,
            json!({"data": {"title": title, "views": views, "status": "LIVE", "author": author_id,
                            "coAuthors": [author_id], "publishedOn": "2026-05-01T12:00:00+02:00",
                            "extra": {"tags": ["a"]}}}),
        )
        .await;
        assert_eq!(res["createPost"]["stage"], "DRAFT");
        assert_eq!(res["createPost"]["author"]["name"], "Writer A");
        post_ids.push(res["createPost"]["id"].as_str().unwrap().to_string());
    }
    for id in &post_ids[..2] {
        ok(&svc, &admin, "mutation($id: ID!) { publishPost(id: $id) { stage } }", json!({"id": id})).await;
    }

    // Public clients see published entries only.
    let public = ok(
        &svc,
        &Principal::Public,
        r#"{ posts(where: {views_gt: 1, title_contains: "o"}, orderBy: [views_DESC]) {
               total items { title views status publishedOn extra ref author { name } coAuthors { name } } } }"#,
        json!({}),
    )
    .await;
    assert_eq!(public["posts"]["total"], 2);
    let items = public["posts"]["items"].as_array().unwrap();
    assert_eq!(items[0]["title"], "World");
    assert_eq!(items[0]["status"], "LIVE");
    assert_eq!(items[0]["publishedOn"], "2026-05-01T10:00:00.000Z");
    assert_eq!(items[0]["extra"], json!({"tags": ["a"]}));
    assert_eq!(items[1]["coAuthors"][0]["name"], "Writer A");

    // Ids are opaque and fresh on every response; refs are stable.
    let again = ok(&svc, &admin, "{ posts(orderBy: [views_DESC]) { items { id ref } } }", json!({})).await;
    let first = &again["posts"]["items"][0];
    assert_ne!(first["id"], json!(post_ids[1]));
    assert_eq!(first["ref"], items[0]["ref"]);
    assert_eq!(find_uuid(&again.to_string()), None, "responses must not leak storage UUIDs");
    assert_eq!(find_uuid(&public.to_string()), None, "responses must not leak storage UUIDs");

    // Filtering by an opaque id and by a relation.
    let by_id = ok(
        &svc,
        &admin,
        "query($ids: [ID!]) { posts(where: {id_in: $ids}, stage: DRAFT) { total } }",
        json!({"ids": [post_ids[0], post_ids[2]]}),
    )
    .await;
    assert_eq!(by_id["posts"]["total"], 2);
    let by_author = ok(
        &svc,
        &admin,
        "query($a: ID) { posts(where: {author: $a}, stage: DRAFT) { total } }",
        json!({"a": author_id}),
    )
    .await;
    assert_eq!(by_author["posts"]["total"], 3);

    // Drafts: editing does not touch the published version.
    ok(
        &svc,
        &admin,
        r#"mutation($id: ID!) { updatePost(id: $id, data: {title: "Hello v2", views: null}) { title views } }"#,
        json!({"id": post_ids[0]}),
    )
    .await;
    let both = ok(
        &svc,
        &admin,
        "query($id: ID!) { d: post(id: $id, stage: DRAFT) { title views } p: post(id: $id) { title views } }",
        json!({"id": post_ids[0]}),
    )
    .await;
    assert_eq!(both["d"], json!({"title": "Hello v2", "views": null}));
    assert_eq!(both["p"], json!({"title": "Hello", "views": 5}));

    // Deleting.
    ok(&svc, &admin, "mutation($id: ID!) { deletePost(id: $id) }", json!({"id": post_ids[2]})).await;
    let res = run(&svc, &admin, "mutation($id: ID!) { deletePost(id: $id) }", json!({"id": post_ids[2]})).await;
    assert_eq!(error_code(&res), "NOT_FOUND");
}

fn find_uuid(s: &str) -> Option<String> {
    s.as_bytes().windows(36).find_map(|w| {
        let w = std::str::from_utf8(w).ok()?;
        uuid::Uuid::parse_str(w).ok().map(|_| w.to_string())
    })
}

#[tokio::test]
async fn authorization() {
    let svc = service();
    ok(&svc, &sys(), SETUP, json!({})).await;

    let res = run(&svc, &Principal::Public, r#"mutation { createAuthor(data: {name: "x"}) { id } }"#, json!({})).await;
    assert_eq!(error_code(&res), "FORBIDDEN");
    let res = run(&svc, &Principal::Public, "{ authors(stage: DRAFT) { total } }", json!({})).await;
    assert_eq!(error_code(&res), "FORBIDDEN");
    let res = run(&svc, &Principal::Public, "{ contentTypes { apiId } }", json!({})).await;
    assert_eq!(error_code(&res), "FORBIDDEN");
    let res = run(&svc, &Principal::Anonymous, "{ authors { total } }", json!({})).await;
    assert_eq!(error_code(&res), "FORBIDDEN");
    ok(&svc, &Principal::Public, "{ authors { total } }", json!({})).await;
}

#[tokio::test]
async fn ids_are_type_checked() {
    let svc = service();
    ok(&svc, &sys(), SETUP, json!({})).await;
    let author = ok(&svc, &sys(), r#"mutation { createAuthor(data: {name: "x"}) { id } }"#, json!({})).await;
    let author_id = &author["createAuthor"]["id"];

    // An Author id used as a Post id.
    let res = run(&svc, &sys(), "query($id: ID!) { post(id: $id, stage: DRAFT) { id } }", json!({"id": author_id})).await;
    assert_eq!(error_code(&res), "BAD_USER_INPUT");
    // Garbage and tampered ids.
    let res = run(&svc, &sys(), r#"{ author(id: "nope", stage: DRAFT) { id } }"#, json!({})).await;
    assert_eq!(error_code(&res), "BAD_USER_INPUT");
    let mut tampered = author_id.as_str().unwrap().to_string();
    let last = tampered.pop().unwrap();
    tampered.push(if last == 'A' { 'B' } else { 'A' });
    let res = run(&svc, &sys(), "query($id: ID!) { author(id: $id, stage: DRAFT) { id } }", json!({"id": tampered})).await;
    assert_eq!(error_code(&res), "BAD_USER_INPUT");
}

#[tokio::test]
async fn schema_evolution_and_conflicts() {
    let svc = service();
    ok(&svc, &sys(), SETUP, json!({})).await;

    // Generated names must not collide.
    let res = run(
        &svc,
        &sys(),
        r#"mutation { createContentType(input: {apiId: "AuthorConnection", fields: [{name: "x", type: INT}]}) { apiId } }"#,
        json!({}),
    )
    .await;
    assert_eq!(error_code(&res), "CONFLICT");
    let res = run(
        &svc,
        &sys(),
        r#"mutation { createContentType(input: {apiId: "Tag", fields: [{name: "ref", type: TEXT}]}) { apiId } }"#,
        json!({}),
    )
    .await;
    assert_eq!(error_code(&res), "BAD_USER_INPUT");

    // Referenced content types cannot be deleted.
    let res = run(&svc, &sys(), r#"mutation { deleteContentType(apiId: "Author") }"#, json!({})).await;
    assert_eq!(error_code(&res), "CONFLICT");

    // A new field is usable right after the change.
    ok(
        &svc,
        &sys(),
        r#"mutation { updateContentType(apiId: "Author", input: {apiId: "Author", fields: [
             {name: "name", type: TEXT, required: true}, {name: "bio", type: TEXT}]}) { fields { name } } }"#,
        json!({}),
    )
    .await;
    let res = ok(&svc, &sys(), r#"mutation { createAuthor(data: {name: "x", bio: "y"}) { bio } }"#, json!({})).await;
    assert_eq!(res["createAuthor"]["bio"], "y");

    ok(&svc, &sys(), r#"mutation { deleteContentType(apiId: "Post") }"#, json!({})).await;
    let res = run(&svc, &sys(), "{ posts { total } }", json!({})).await;
    assert!(res["errors"][0]["message"].as_str().unwrap().contains("posts"), "{res}");
}

#[tokio::test]
async fn media_assets() {
    use tess_graphql::Services;
    use tess_media::{FsBlobStore, MediaConfig, MediaService};

    let dir = tempfile::tempdir().unwrap();
    let codec = IdCodec::new(IdConfig {
        secret: random_secret(),
        previous_secrets: vec![],
        ref_secret: random_secret(),
        ttl: Duration::from_secs(3600),
    })
    .unwrap();
    let media = MediaService::new(Arc::new(FsBlobStore::new(dir.path())), MediaConfig::default());
    let svc = GraphqlService::with_services(
        Arc::new(MemoryStorage::new()),
        codec,
        SchemaConfig::default(),
        Services { media: Some(media.clone()), ..Default::default() },
    );
    svc.install().await.unwrap();

    let asset = media.upload(svc.cms(), &tess_core::Actor::system(), "a.txt", "text/plain", bytes::Bytes::from_static(b"hi")).await.unwrap();
    let id = svc.codec().encode("Asset", asset.id);
    ok(
        &svc,
        &sys(),
        r#"mutation { createContentType(input: {apiId: "Page", fields: [
             {name: "title", type: TEXT}, {name: "cover", type: RELATION, relationTarget: "Asset"}]}) { apiId } }"#,
        json!({}),
    )
    .await;
    let page = ok(
        &svc,
        &sys(),
        "mutation($c: ID) { createPage(data: {title: \"p\", cover: $c}) { cover { fileName mimeType size status url } } }",
        json!({"c": id}),
    )
    .await;
    let cover = &page["createPage"]["cover"];
    assert_eq!(cover["fileName"], "a.txt");
    assert_eq!(cover["status"], "READY");
    assert!(cover["url"].as_str().unwrap().starts_with("/media/files/assets/"));

    // Direct uploads need a presigning store; the built-in type is protected.
    let res = run(&svc, &sys(), r#"mutation { createAssetUpload(fileName: "x", mimeType: "text/plain", size: 1) { url } }"#, json!({})).await;
    assert_eq!(error_code(&res), "BAD_USER_INPUT");
    let res = run(&svc, &sys(), r#"mutation { deleteContentType(apiId: "Asset") }"#, json!({})).await;
    assert_eq!(error_code(&res), "CONFLICT");
}

#[tokio::test]
async fn full_text_search() {
    use tess_graphql::Services;
    use tess_search::{MemoryEngine, SearchService};

    let codec = IdCodec::new(IdConfig {
        secret: random_secret(),
        previous_secrets: vec![],
        ref_secret: random_secret(),
        ttl: Duration::from_secs(3600),
    })
    .unwrap();
    let storage: Arc<dyn tess_core::Storage> = Arc::new(MemoryStorage::new());
    let search = SearchService::new(Arc::new(MemoryEngine::default()), storage.clone());
    let svc = GraphqlService::with_services(storage, codec, SchemaConfig::default(), Services { search: Some(search), ..Default::default() });
    svc.install().await.unwrap();
    ok(&svc, &sys(), SETUP, json!({})).await;
    for title in ["Rust in production", "Gardening tips", "Rusty bikes"] {
        let r = ok(&svc, &sys(), "mutation($t: String!) { createPost(data: {title: $t}) { id } }", json!({"t": title})).await;
        if title != "Rusty bikes" {
            ok(&svc, &sys(), "mutation($id: ID!) { publishPost(id: $id) { id } }", json!({"id": r["createPost"]["id"]})).await;
        }
    }
    let res = ok(&svc, &Principal::Public, r#"{ searchPosts(query: "rust") { total items { title } } }"#, json!({})).await;
    assert_eq!(res["searchPosts"], json!({"total": 1, "items": [{"title": "Rust in production"}]}));
    let res = ok(&svc, &sys(), r#"{ searchPosts(query: "rust", stage: DRAFT) { total } }"#, json!({})).await;
    assert_eq!(res["searchPosts"]["total"], 2);
    let res = run(&svc, &Principal::Public, r#"{ searchPosts(query: "rust", stage: DRAFT) { total } }"#, json!({})).await;
    assert_eq!(error_code(&res), "FORBIDDEN");
    let res = ok(&svc, &sys(), r#"mutation { reindexSearch(apiId: "Post") }"#, json!({})).await;
    assert_eq!(res["reindexSearch"], 3);
}
