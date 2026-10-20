// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Minimal typed Rust client for the headless CMS GraphQL API.
//!
//!     TESS_URL=http://localhost:8080 cargo run
//!
//! Lists, searches and fetches posts by slug, shows GraphQL error codes and,
//! when `TESS_ADMIN_TOKEN` is set, creates + publishes a post and uploads a file.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
struct GraphQLError {
    message: String,
    #[serde(default)]
    extensions: Option<ErrorExtensions>,
}

#[derive(Debug, Deserialize)]
struct ErrorExtensions {
    code: Option<String>,
}

#[derive(Debug)]
enum CmsError {
    Http(reqwest::Error),
    GraphQL { code: String, message: String },
    Upload { status: u16, body: String },
}

impl std::fmt::Display for CmsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CmsError::Http(e) => write!(f, "http error: {e}"),
            CmsError::GraphQL { code, message } => write!(f, "[{code}] {message}"),
            CmsError::Upload { status, body } => write!(f, "upload failed (HTTP {status}): {body}"),
        }
    }
}

impl std::error::Error for CmsError {}

impl From<reqwest::Error> for CmsError {
    fn from(e: reqwest::Error) -> Self {
        CmsError::Http(e)
    }
}

#[derive(Deserialize)]
struct Response<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GraphQLError>,
}

#[derive(Debug, Deserialize)]
struct Author {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Post {
    /// Opaque, encrypted and expiring: use it immediately, never persist it.
    #[serde(default)]
    id: Option<String>,
    /// Stable opaque reference: use it for cache keys.
    r#ref: String,
    title: String,
    slug: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    tags: Option<Value>,
    #[serde(default)]
    stage: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    author: Option<Author>,
}

#[derive(Debug, Deserialize)]
struct PostConnection {
    total: u64,
    items: Vec<Post>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Asset {
    r#ref: String,
    file_name: String,
    mime_type: String,
    size: u64,
    url: String,
}

#[derive(Serialize)]
struct Request<'a> {
    query: &'a str,
    variables: Value,
}

struct Client {
    http: reqwest::Client,
    base: String,
    admin_token: Option<String>,
}

impl Client {
    async fn gql<T: DeserializeOwned>(
        &self,
        query: &str,
        variables: Value,
        admin: bool,
    ) -> Result<T, CmsError> {
        let mut req = self
            .http
            .post(format!("{}/graphql", self.base))
            .json(&Request { query, variables });
        if admin && let Some(token) = &self.admin_token {
            req = req.bearer_auth(token);
        }
        let res: Response<T> = req.send().await?.error_for_status()?.json().await?;
        if let Some(e) = res.errors.into_iter().next() {
            let code = e
                .extensions
                .and_then(|x| x.code)
                .unwrap_or_else(|| "UNKNOWN".into());
            return Err(CmsError::GraphQL {
                code,
                message: e.message,
            });
        }
        res.data.ok_or_else(|| CmsError::GraphQL {
            code: "NO_DATA".into(),
            message: "empty response".into(),
        })
    }

    async fn post_by_slug(&self, slug: &str) -> Result<Option<Post>, CmsError> {
        #[derive(Deserialize)]
        struct D {
            posts: PostConnection,
        }
        let d: D = self
            .gql(POST_BY_SLUG, json!({ "slug": slug }), false)
            .await?;
        Ok(d.posts.items.into_iter().next())
    }

    async fn upload(&self, file_name: &str, mime: &str, bytes: Vec<u8>) -> Result<Asset, CmsError> {
        let token = self.admin_token.as_deref().unwrap_or_default();
        let res = self
            .http
            .post(format!("{}/media/upload", self.base))
            .query(&[("fileName", file_name)])
            .header(reqwest::header::CONTENT_TYPE, mime)
            .bearer_auth(token)
            .body(bytes)
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(CmsError::Upload {
                status: res.status().as_u16(),
                body: res.text().await.unwrap_or_default(),
            });
        }
        let mut asset: Asset = res.json().await?;
        if asset.url.starts_with('/') {
            asset.url = format!("{}{}", self.base, asset.url); // filesystem store: server-relative URL
        }
        Ok(asset)
    }
}

const LIST_POSTS: &str = "query ListPosts($first: Int!) {
  posts(orderBy: [createdAt_DESC], first: $first) { total items { ref title slug publishedAt author { name } } }
}";

const SEARCH_POSTS: &str = "query Search($q: String!) {
  searchPosts(query: $q, first: 5) { total items { ref title slug } }
}";

/// Route by slug: ids are encrypted and expire.
const POST_BY_SLUG: &str = "query PostBySlug($slug: String!) {
  posts(where: { slug: $slug }, first: 1) { total items { ref title slug body tags publishedAt author { name } } }
}";

const CREATE_POST: &str = "mutation Create($data: PostCreateInput!) { createPost(data: $data) { id ref title slug stage } }";
const PUBLISH_POST: &str =
    "mutation Publish($id: ID!) { publishPost(id: $id) { ref title slug stage publishedAt } }";

#[derive(Deserialize)]
struct ListData {
    posts: PostConnection,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchData {
    search_posts: PostConnection,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateData {
    create_post: Post,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishData {
    publish_post: Post,
}

fn author_name(p: &Post) -> &str {
    p.author.as_ref().map_or("?", |a| a.name.as_str())
}

async fn admin_demo(c: &Client) -> Result<(), CmsError> {
    if c.admin_token.is_none() {
        println!("\n(admin) TESS_ADMIN_TOKEN not set: skipping create/publish and media upload");
        return Ok(());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let slug = format!("rust-example-{}", now.as_nanos());
    let data = json!({
        "title": "Hello from Rust", "slug": slug, "excerpt": "Created by examples/clients/rust.",
        "body": "Draft first, then published.", "tags": ["example", "rust"],
    });
    let created: CreateData = c.gql(CREATE_POST, json!({ "data": data }), true).await?;
    let draft = created.create_post;
    println!(
        "\n(admin) created draft {} stage={} ref={}",
        draft.slug.as_deref().unwrap_or_default(),
        draft.stage.as_deref().unwrap_or_default(),
        draft.r#ref
    );
    // Use the id right away; it is never stored.
    let id = draft.id.unwrap_or_default();
    let published: PublishData = c.gql(PUBLISH_POST, json!({ "id": id }), true).await?;
    let p = published.publish_post;
    println!(
        "(admin) published {} stage={} at {}",
        p.slug.as_deref().unwrap_or_default(),
        p.stage.as_deref().unwrap_or_default(),
        p.published_at.as_deref().unwrap_or_default()
    );

    let content = format!(
        "generated by the rust example ({}s since epoch)\n",
        now.as_secs()
    )
    .into_bytes();
    let asset = c.upload("rust-example.txt", "text/plain", content).await?;
    println!(
        "(admin) uploaded {} ({}, {} bytes) ref={}\n(admin) download: {}",
        asset.file_name, asset.mime_type, asset.size, asset.r#ref, asset.url
    );
    Ok(())
}

async fn run(c: &Client) -> Result<(), CmsError> {
    let list: ListData = c.gql(LIST_POSTS, json!({ "first": 10 }), false).await?;
    println!("{} published post(s):", list.posts.total);
    for p in &list.posts.items {
        let slug = p.slug.as_deref().unwrap_or_default();
        println!(
            "  - [{}] {} (/posts/{slug}) by {}",
            p.r#ref,
            p.title,
            author_name(p)
        );
    }

    let search: SearchData = c.gql(SEARCH_POSTS, json!({ "q": "lambda" }), false).await?;
    let slugs: Vec<_> = search
        .search_posts
        .items
        .iter()
        .filter_map(|p| p.slug.as_deref())
        .collect();
    println!(
        "\nsearch 'lambda': {} hit(s): {slugs:?}",
        search.search_posts.total
    );

    if let Some(slug) = list.posts.items.first().and_then(|p| p.slug.as_deref())
        && let Some(post) = c.post_by_slug(slug).await?
    {
        println!(
            "\nby slug '{slug}': {} tags={} published={}\n  {}",
            post.title,
            post.tags.unwrap_or(Value::Null),
            post.published_at.as_deref().unwrap_or("-"),
            post.body.as_deref().unwrap_or_default()
        );
    }
    println!(
        "by slug 'does-not-exist': {:?}",
        c.post_by_slug("does-not-exist").await?.map(|p| p.r#ref)
    );

    match c
        .gql::<Value>(
            "query($id: ID!) { post(id: $id) { title } }",
            json!({ "id": "not-a-real-id" }),
            false,
        )
        .await
    {
        Err(CmsError::GraphQL { code, message }) => {
            println!("\nexpected error, code={code}: {message}")
        }
        other => println!("\nunexpected: {other:?}"),
    }
    match c
        .gql::<Value>(CREATE_POST, json!({ "data": { "title": "nope" } }), false)
        .await
    {
        Err(CmsError::GraphQL { code, message }) => {
            println!("expected error, code={code}: {message}")
        }
        other => println!("unexpected: {other:?}"),
    }

    admin_demo(c).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = std::env::var("TESS_URL").unwrap_or_else(|_| "http://localhost:8080".into());
    let client = Client {
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?,
        base: base.trim_end_matches('/').to_string(),
        admin_token: std::env::var("TESS_ADMIN_TOKEN")
            .ok()
            .filter(|t| !t.is_empty()),
    };
    run(&client).await?;
    Ok(())
}
