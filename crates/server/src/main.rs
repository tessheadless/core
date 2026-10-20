// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! CMS server: one binary that runs as a regular HTTP server (Docker, VM) or,
//! when it detects the Lambda runtime, as an AWS Lambda function.

mod cache;
mod config;
mod integrations;
mod routes;

use std::sync::Arc;

use anyhow::Result;
use async_graphql_axum::GraphQLRequest;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use tess_graphql::async_graphql::http::GraphiQLSource;
use tess_graphql::{GraphqlService, Principal, Services};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::config::{Config, running_on_lambda};

pub(crate) struct AppState {
    service: Arc<GraphqlService>,
    storage_name: &'static str,
    /// SHA-256 of the admin token, compared in constant time.
    admin_token_hash: Option<[u8; 32]>,
    auth: Arc<tess_auth::AuthService>,
    public_read: bool,
    n8n: Option<integrations::N8nProxy>,
    cache: Option<tess_cache::CacheService>,
}

impl AppState {
    /// Resolves `Authorization: Bearer …`: a user session, an API token, or
    /// the bootstrap admin token (full access, as the "system" Owner).
    pub(crate) async fn principal(&self, headers: &HeaderMap) -> Result<Principal, StatusCode> {
        let Some(auth) = headers.get(header::AUTHORIZATION) else {
            return Ok(if self.public_read { Principal::Public } else { Principal::Anonymous });
        };
        let token = auth
            .to_str()
            .ok()
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .ok_or(StatusCode::UNAUTHORIZED)?;
        match self.auth.authenticate(token).await {
            Ok(Some(actor)) => return Ok(Principal::User(actor)),
            Ok(None) => {}
            Err(tess_core::CmsError::Forbidden(_)) => return Err(StatusCode::UNAUTHORIZED),
            Err(e) => {
                tracing::error!(error = %e, "authentication failed");
                return Err(StatusCode::SERVICE_UNAVAILABLE);
            }
        }
        let given: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        match &self.admin_token_hash {
            Some(expected) if bool::from(given.ct_eq(expected)) => Ok(Principal::User(tess_core::Actor::system())),
            _ => Err(StatusCode::UNAUTHORIZED),
        }
    }
}

/// Authenticates the bearer token of `/mcp` and passes the resulting
/// [`Principal`] to the MCP tools through the request extensions. Anonymous
/// callers still reach the tools (public reads), exactly like `/graphql`.
async fn mcp_auth(
    State(state): State<Arc<AppState>>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    match state.principal(req.headers()).await {
        Ok(principal) => {
            req.extensions_mut().insert(principal);
            next.run(req).await
        }
        Err(status) => (status, Json(serde_json::json!({"error": "invalid credentials"}))).into_response(),
    }
}

async fn graphql(State(state): State<Arc<AppState>>, headers: HeaderMap, req: GraphQLRequest) -> Response {
    match state.principal(&headers).await {
        Ok(principal) => cache::execute(&state.service, state.cache.as_ref(), req.into_inner(), principal).await,
        Err(status) => (status, Json(serde_json::json!({"errors": [{"message": "invalid credentials"}]}))).into_response(),
    }
}

/// The Tess logo, embedded in the binary (source: `assets/Tess-logo.svg`).
const LOGO_SVG: &str = include_str!("../../../assets/Tess-logo.svg");

async fn favicon() -> Response {
    (
        [(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "public, max-age=86400")],
        LOGO_SVG,
    )
        .into_response()
}

async fn graphiql() -> Html<String> {
    let page = GraphiQLSource::build().endpoint("/graphql").title("Tess · GraphiQL").finish();
    Html(page.replacen("</head>", r#"<link rel="icon" type="image/svg+xml" href="/favicon.svg"></head>"#, 1))
}

async fn sdl(State(state): State<Arc<AppState>>) -> Response {
    match state.service.sdl().await {
        Ok(sdl) => ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], sdl).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "cannot render schema");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

async fn health(State(state): State<Arc<AppState>>) -> Response {
    let status = match state.service.cms().content_types().await {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            tracing::error!(error = %e, "health check failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    };
    let body = serde_json::json!({
        "status": if status.is_success() { "ok" } else { "unavailable" },
        "storage": state.storage_name,
        "cache": state.cache.as_ref().map(|c| c.name()),
    });
    (status, Json(body)).into_response()
}

fn cors(origins: &[String]) -> CorsLayer {
    let layer = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
    if origins.iter().any(|o| o == "*") {
        layer.allow_origin(Any)
    } else {
        layer.allow_origin(AllowOrigin::list(origins.iter().filter_map(|o| HeaderValue::from_str(o).ok())))
    }
}

fn router(state: Arc<AppState>, config: &Config, limits: &BodyLimits) -> Router {
    let graphql_route = if config.graphiql { get(graphiql).post(graphql) } else { axum::routing::post(graphql) };
    let mut app = Router::new()
        .route("/graphql", graphql_route)
        .route("/graphql/schema.graphql", get(sdl))
        .route("/health", get(health))
        .route("/favicon.svg", get(favicon))
        .route(
            "/media/upload",
            axum::routing::post(routes::upload_media).layer(DefaultBodyLimit::max(limits.media)),
        )
        .route("/media/files/{*key}", get(routes::serve_media))
        .route(
            "/api/plugins/{name}",
            axum::routing::post(routes::upload_plugin).layer(DefaultBodyLimit::max(limits.plugin)),
        )
        .route("/i18n/{format}/{*path}", get(integrations::serve_translations))
        .route("/api/i18n/export", get(integrations::download_export));
    if let Some(proxy) = &state.n8n {
        // n8n runs with N8N_PATH = this path; the prefix is stripped when forwarding.
        let path = proxy.path().to_string();
        app = app
            .route(&path, axum::routing::any(integrations::proxy_n8n))
            .route(&format!("{path}/"), axum::routing::any(integrations::proxy_n8n))
            .route(&format!("{path}/{{*rest}}"), axum::routing::any(integrations::proxy_n8n));
        tracing::info!(path, "proxying the n8n workflow editor");
    }
    if config.mcp {
        // Stateless MCP over Streamable HTTP. `principal` comes from the
        // middleware below, so the tools see the same access rules as /graphql.
        // A sub-router, so the auth layer only wraps /mcp and not the other routes.
        let mcp = tess_mcp::http_service(state.service.clone(), config.mcp_allowed_hosts.clone());
        let mcp_routes = Router::new()
            .route_service("/mcp", mcp)
            .route_layer(axum::middleware::from_fn_with_state(state.clone(), mcp_auth));
        app = app.merge(mcp_routes);
        tracing::info!("MCP endpoint enabled at /mcp");
    }
    if let Some(dir) = &config.admin_dir {
        // Client-side routes (e.g. /admin/content/Post/…) fall back to the SPA shell.
        let spa = ServeDir::new(dir).fallback(ServeFile::new(format!("{dir}/index.html")));
        let admin = Router::new()
            .fallback_service(spa)
            .layer(axum::middleware::map_request(admin_dir_slash));
        app = app.nest("/admin", admin);
        // `nest` matches `/admin` and `/admin/…`, but not `/admin/` itself.
        app = app.route_service("/admin/", ServeFile::new(format!("{dir}/index.html")));
        tracing::info!(dir, "serving admin UI at /admin");
    }
    app.with_state(state).layer(cors(&config.cors_origins)).layer(TraceLayer::new_for_http())
}

/// Serve prerendered pages (`/content` → `/content/index.html`) without a redirect.
///
/// `ServeDir` answers a directory path lacking its trailing slash with a redirect
/// built from the nested path, which drops the `/admin` prefix: reloading
/// `/admin/content` would land on `/content/` (404). Rewriting the path in place
/// avoids the redirect altogether.
async fn admin_dir_slash(mut req: axum::extract::Request) -> axum::extract::Request {
    let uri = req.uri();
    let path = uri.path();
    let last = path.rsplit('/').next().unwrap_or_default();
    if !path.ends_with('/') && !last.contains('.') {
        let rewritten = match uri.query() {
            Some(q) => format!("{path}/?{q}"),
            None => format!("{path}/"),
        };
        if let Ok(new_uri) = rewritten.parse() {
            *req.uri_mut() = new_uri;
        }
    }
    req
}

struct BodyLimits {
    media: usize,
    plugin: usize,
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let json = running_on_lambda() || std::env::var("TESS_LOG_FORMAT").is_ok_and(|v| v == "json");
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if json {
        builder.json().without_time().init(); // CloudWatch adds timestamps
    } else {
        builder.init();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        // Local MCP proxy for desktop agents. It talks to a running server over
        // HTTP and needs no database or server secrets. Logs go to stderr so
        // they never corrupt the stdout protocol stream.
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
            .init();
        let url = std::env::var("TESS_MCP_URL").unwrap_or_else(|_| "http://localhost:8080".into());
        let token = std::env::var("TESS_MCP_TOKEN").ok().filter(|t| !t.trim().is_empty());
        return tess_mcp::run_stdio(tess_mcp::RemoteApi::new(url, token)).await;
    }
    init_tracing();
    let config = Config::from_env()?;
    let codec = config::id_codec()?;
    let storage = config::storage().await?;
    let blobs = config::blob_store().await?;
    let media_config = config::media_config()?;
    let limits = BodyLimits {
        media: usize::try_from(media_config.max_upload_bytes).unwrap_or(usize::MAX),
        plugin: config::plugin_limits()?.map_or(0, |l| l.max_module_bytes),
    };
    let ref_codec = Arc::new(config::id_codec()?);
    let cache = config::cache_service(media_config.url_ttl, codec.ttl())?;
    let auth = config::auth_service(storage.clone())?;
    let secrets = config::secrets_key()?;
    let n8n = config::n8n_config()?;
    let webhooks = match config::webhook_config()? {
        Some(cfg) => {
            let (ids, refs) = (Arc::new(config::id_codec()?), ref_codec.clone());
            Some(Arc::new(tess_webhooks::WebhookService::new(
                storage.clone(),
                secrets.clone(),
                Arc::new(move |ct, id| ids.encode(ct, id)),
                Arc::new(move |ct, id| refs.stable_ref(ct, id)),
                cfg,
            )))
        }
        None => None,
    };
    let services = Services {
        auth: Some(auth.clone()),
        media: Some(tess_media::MediaService::new(blobs.clone(), media_config)),
        plugins: config::plugin_limits()?.map(|limits| {
            let refs: tess_plugins::RefFn = Arc::new(move |ct, id| ref_codec.stable_ref(ct, id));
            Arc::new(tess_plugins::PluginManager::new(storage.clone(), blobs.clone(), refs, limits))
        }),
        search: config::search_engine()?.map(|engine| tess_search::SearchService::new(engine, storage.clone())),
        i18n: config::i18n_config()?
            .map(|cfg| Arc::new(tess_i18n::I18nService::new(storage.clone(), secrets.clone(), cfg))),
        webhooks,
        workflows: n8n.as_ref().map(|n| tess_graphql::WorkflowsInfo {
            provider: "n8n".into(),
            editor_url: format!("{}/", n.path),
        }),
    };
    if config.admin_token.is_some() {
        tracing::warn!("TESS_ADMIN_TOKEN is set: it grants full access; remove it once an Owner account exists");
    }
    let state = Arc::new(AppState {
        storage_name: storage.name(),
        service: Arc::new(GraphqlService::with_services(storage, codec, config.schema.clone(), services)),
        admin_token_hash: config.admin_token.as_ref().map(|t| Sha256::digest(t.as_bytes()).into()),
        auth,
        public_read: config.public_read,
        n8n: n8n.map(integrations::N8nProxy::new),
        cache,
    });
    state.service.install().await?;
    if let Some(cache) = &state.cache {
        state.service.cms().hooks().add(Arc::new(cache.hook()));
        // Another instance changed the content model: rebuild the schema now.
        let service = state.service.clone();
        cache.on_schema_change(move || service.mark_schema_dirty());
        tracing::info!(cache = cache.name(), ttl_secs = cache.config().ttl.as_secs(), "response cache enabled");
    }
    // Fail fast if the stored content model cannot produce a schema.
    state.service.schema().await?;
    let app = router(state.clone(), &config, &limits);

    if running_on_lambda() {
        #[cfg(feature = "lambda")]
        {
            tracing::info!(storage = state.storage_name, "starting in AWS Lambda mode");
            return lambda_http::run(app).await.map_err(|e| anyhow::anyhow!(e));
        }
        #[cfg(not(feature = "lambda"))]
        anyhow::bail!("running on AWS Lambda but built without the `lambda` feature");
    }

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    tracing::info!(port = config.port, storage = state.storage_name, "listening");
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
}
