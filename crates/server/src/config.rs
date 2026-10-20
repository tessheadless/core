// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Configuration from environment variables. Secrets are only ever read from
//! the environment (inject them from your orchestrator / AWS Secrets Manager),
//! never from files in the repository.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tess_core::Storage;
use tess_graphql::SchemaConfig;
use tess_graphql::ids::{IdCodec, IdConfig, parse_ttl};

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn var_or(name: &str, default: &str) -> String {
    var(name).unwrap_or_else(|| default.to_string())
}

fn parse_var<T: std::str::FromStr>(name: &str, default: T) -> Result<T> {
    match var(name) {
        Some(v) => v
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid value for {name}: `{v}`")),
        None => Ok(default),
    }
}

fn bool_var(name: &str, default: bool) -> Result<bool> {
    match var(name).as_deref().map(str::to_ascii_lowercase).as_deref() {
        None => Ok(default),
        Some("1" | "true" | "yes" | "on") => Ok(true),
        Some("0" | "false" | "no" | "off") => Ok(false),
        Some(v) => bail!("invalid boolean for {name}: `{v}`"),
    }
}

pub fn running_on_lambda() -> bool {
    std::env::var_os("AWS_LAMBDA_RUNTIME_API").is_some()
}

pub struct Config {
    pub port: u16,
    pub admin_token: Option<String>,
    pub public_read: bool,
    pub graphiql: bool,
    /// Serve the MCP endpoint at `/mcp` (`TESS_MCP`, off by default).
    pub mcp: bool,
    /// Hosts accepted in the `Host` header of `/mcp` (`TESS_MCP_ALLOWED_HOSTS`).
    pub mcp_allowed_hosts: Vec<String>,
    pub cors_origins: Vec<String>,
    pub admin_dir: Option<String>,
    pub schema: SchemaConfig,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let admin_token = var("TESS_ADMIN_TOKEN");
        if let Some(t) = &admin_token
            && t.len() < 32
        {
            bail!("TESS_ADMIN_TOKEN must be at least 32 characters");
        }
        let defaults = SchemaConfig::default();
        Ok(Config {
            port: parse_var("PORT", 8080)?,
            admin_token,
            public_read: bool_var("TESS_PUBLIC_READ", true)?,
            graphiql: bool_var("TESS_GRAPHIQL", true)?,
            mcp: bool_var("TESS_MCP", false)?,
            mcp_allowed_hosts: var_or("TESS_MCP_ALLOWED_HOSTS", "localhost,127.0.0.1,::1")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            cors_origins: var_or("TESS_CORS_ORIGINS", "*")
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            admin_dir: var("TESS_ADMIN_DIR").or_else(default_admin_dir),
            schema: SchemaConfig {
                default_page_size: parse_var("TESS_DEFAULT_PAGE_SIZE", defaults.default_page_size)?,
                max_page_size: parse_var("TESS_MAX_PAGE_SIZE", defaults.max_page_size)?,
                max_depth: parse_var("TESS_MAX_QUERY_DEPTH", defaults.max_depth)?,
                introspection: bool_var("TESS_INTROSPECTION", defaults.introspection)?,
                refresh_interval: Duration::from_secs(parse_var(
                    "TESS_SCHEMA_REFRESH_SECS",
                    defaults.refresh_interval.as_secs(),
                )?),
            },
        })
    }
}

/// The admin UI of a source checkout (`admin/dist/client`), else the `admin/`
/// folder next to the binary (release archives).
fn default_admin_dir() -> Option<String> {
    let mut candidates = vec![std::path::PathBuf::from("admin/dist/client")];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(std::path::Path::parent)
    {
        candidates.push(dir.join("admin"));
    }
    candidates
        .into_iter()
        .find(|d| d.join("index.html").exists())
        .map(|d| d.to_string_lossy().into_owned())
}

/// Builds the opaque-id codec. Refuses to start without proper secrets.
pub fn id_codec() -> Result<IdCodec> {
    let secret =
        var("TESS_ID_SECRET").context("TESS_ID_SECRET is required (>= 32 bytes, random)")?;
    let ref_secret =
        var("TESS_REF_SECRET").context("TESS_REF_SECRET is required (>= 32 bytes, random)")?;
    let previous_secrets = var("TESS_ID_SECRET_PREVIOUS")
        .map(|v| v.split(',').map(|s| s.trim().as_bytes().to_vec()).collect())
        .unwrap_or_default();
    let ttl = parse_ttl(&var_or("TESS_ID_TTL", "24h"))?;
    Ok(IdCodec::new(IdConfig {
        secret: secret.into_bytes(),
        previous_secrets,
        ref_secret: ref_secret.into_bytes(),
        ttl,
    })?)
}

/// User accounts and sessions. Refuses to start without a session secret.
pub fn auth_service(storage: Arc<dyn Storage>) -> Result<Arc<tess_auth::AuthService>> {
    let secret = var("TESS_SESSION_SECRET")
        .context("TESS_SESSION_SECRET is required (>= 32 bytes, random)")?;
    let key = tess_auth::SessionKey::new(secret.as_bytes())
        .context("TESS_SESSION_SECRET must be at least 32 bytes")?;
    let ttl = parse_ttl(&var_or("TESS_SESSION_TTL", "12h"))?;
    Ok(Arc::new(tess_auth::AuthService::new(storage, key, ttl)))
}

/// Connects the backend selected by `TESS_DATABASE`.
pub async fn storage() -> Result<Arc<dyn Storage>> {
    let kind = var_or("TESS_DATABASE", "memory");
    let storage: Arc<dyn Storage> = match kind.as_str() {
        "memory" => {
            tracing::warn!("using the in-memory backend: all data is lost on restart");
            Arc::new(tess_core::memory::MemoryStorage::new())
        }
        #[cfg(feature = "sql")]
        "postgres" | "mysql" | "sqlite" => {
            let url = var("DATABASE_URL")
                .with_context(|| format!("DATABASE_URL is required for {kind}"))?;
            let dialect = tess_storage_sql::Dialect::from_url(&url);
            let expected = match kind.as_str() {
                "postgres" => tess_storage_sql::Dialect::Postgres,
                "mysql" => tess_storage_sql::Dialect::MySql,
                _ => tess_storage_sql::Dialect::Sqlite,
            };
            if dialect != Some(expected) {
                bail!("DATABASE_URL does not match TESS_DATABASE={kind}");
            }
            if expected == tess_storage_sql::Dialect::Sqlite && running_on_lambda() {
                bail!("sqlite is a local embedded database and cannot be used on AWS Lambda");
            }
            let default_pool = if running_on_lambda() { 2 } else { 10 };
            let max = parse_var("TESS_DB_MAX_CONNECTIONS", default_pool)?;
            Arc::new(tess_storage_sql::SqlStorage::connect(&url, max).await?)
        }
        #[cfg(feature = "mongodb")]
        "mongodb" => {
            let url = var("MONGODB_URL").context("MONGODB_URL is required for mongodb")?;
            let db = var_or("MONGODB_DATABASE", "tess");
            Arc::new(tess_storage_mongodb::MongoStorage::connect(&url, &db).await?)
        }
        #[cfg(feature = "dynamodb")]
        "dynamodb" => {
            let table = var_or("DYNAMODB_TABLE", "tess");
            let endpoint = var("DYNAMODB_ENDPOINT");
            let create = bool_var("DYNAMODB_CREATE_TABLE", false)?;
            Arc::new(
                tess_storage_dynamodb::DynamoDbStorage::connect(
                    &table,
                    endpoint.as_deref(),
                    create,
                )
                .await,
            )
        }
        #[cfg(feature = "rocksdb")]
        "rocksdb" => {
            if running_on_lambda() {
                bail!("rocksdb is a local embedded database and cannot be used on AWS Lambda");
            }
            Arc::new(tess_storage_rocksdb::RocksDbStorage::open(var_or(
                "ROCKSDB_PATH",
                "data/rocksdb",
            ))?)
        }
        other => bail!(
            "unsupported TESS_DATABASE `{other}` (memory, postgres, mysql, sqlite, mongodb, dynamodb, rocksdb; \
             a backend is only available when compiled with its cargo feature)"
        ),
    };
    storage
        .migrate()
        .await
        .with_context(|| format!("{kind}: migration failed"))?;
    Ok(storage)
}

/// Blob store selected by `TESS_MEDIA_STORE` (`fs` or `s3`).
pub async fn blob_store() -> Result<Arc<dyn tess_media::BlobStore>> {
    let default = if running_on_lambda() { "s3" } else { "fs" };
    match var_or("TESS_MEDIA_STORE", default).as_str() {
        "fs" => {
            if running_on_lambda() {
                bail!("TESS_MEDIA_STORE=fs is not persistent on AWS Lambda; use s3");
            }
            Ok(Arc::new(tess_media::FsBlobStore::new(var_or(
                "TESS_MEDIA_DIR",
                "data/media",
            ))))
        }
        #[cfg(feature = "s3")]
        "s3" => {
            let credentials = match (
                var("TESS_S3_ACCESS_KEY_ID"),
                var("TESS_S3_SECRET_ACCESS_KEY"),
            ) {
                (Some(id), Some(secret)) => Some((id, secret)),
                (None, None) => None, // default AWS chain (IAM role, env, profile)
                _ => bail!(
                    "set both TESS_S3_ACCESS_KEY_ID and TESS_S3_SECRET_ACCESS_KEY, or neither"
                ),
            };
            let endpoint = var("TESS_S3_ENDPOINT");
            let store = tess_media::S3BlobStore::connect(tess_media::S3Config {
                bucket: var("TESS_S3_BUCKET")
                    .context("TESS_S3_BUCKET is required for the s3 media store")?,
                force_path_style: bool_var("TESS_S3_FORCE_PATH_STYLE", endpoint.is_some())?,
                endpoint,
                // Where browsers reach the store, for presigned URLs.
                public_endpoint: var("TESS_S3_PUBLIC_ENDPOINT"),
                region: var("TESS_S3_REGION"),
                credentials,
            })
            .await;
            if bool_var("TESS_S3_CREATE_BUCKET", false)? {
                store.ensure_bucket().await?;
            }
            Ok(Arc::new(store))
        }
        other => bail!("unsupported TESS_MEDIA_STORE `{other}` (fs, s3)"),
    }
}

pub fn media_config() -> Result<tess_media::MediaConfig> {
    let defaults = tess_media::MediaConfig::default();
    Ok(tess_media::MediaConfig {
        public_base_url: var("TESS_MEDIA_PUBLIC_URL"),
        url_ttl: Duration::from_secs(parse_var(
            "TESS_MEDIA_URL_TTL_SECS",
            defaults.url_ttl.as_secs(),
        )?),
        max_upload_bytes: parse_var("TESS_MEDIA_MAX_UPLOAD_BYTES", defaults.max_upload_bytes)?,
        proxy_path: defaults.proxy_path,
    })
}

/// Search engine selected by `TESS_SEARCH` (`none`, `memory`, `typesense`).
pub fn search_engine() -> Result<Option<Arc<dyn tess_search::SearchEngine>>> {
    match var_or("TESS_SEARCH", "none").as_str() {
        "none" => Ok(None),
        "memory" => Ok(Some(Arc::new(tess_search::MemoryEngine::default()))),
        "typesense" => Ok(Some(Arc::new(tess_search::TypesenseEngine::new(
            tess_search::TypesenseConfig {
                url: var("TYPESENSE_URL")
                    .context("TYPESENSE_URL is required for typesense search")?,
                api_key: var("TYPESENSE_API_KEY")
                    .context("TYPESENSE_API_KEY is required for typesense search")?,
                collection_prefix: var_or("TYPESENSE_COLLECTION_PREFIX", "tess_"),
            },
        )?))),
        other => bail!("unsupported TESS_SEARCH `{other}` (none, memory, typesense)"),
    }
}

/// Response cache selected by `TESS_CACHE`: `none`, `memory`, or a server
/// speaking the Redis protocol at `TESS_CACHE_URL` (`valkey`, `redis`,
/// `keydb`, `dragonfly`: one client, the name shows in logs and `/health`).
/// Signed media URLs and opaque ids in cached responses must outlive them, so
/// the TTL is capped at half the shortest of the two lifetimes.
pub fn cache_service(
    media_url_ttl: Duration,
    id_ttl: Duration,
) -> Result<Option<tess_cache::CacheService>> {
    let store: Arc<dyn tess_cache::CacheStore> = match var_or("TESS_CACHE", "none").as_str() {
        "none" => return Ok(None),
        "memory" => Arc::new(tess_cache::MemoryStore::new(parse_var(
            "TESS_CACHE_MAX_ENTRIES",
            10_000,
        )?)),
        kind @ ("valkey" | "redis" | "keydb" | "dragonfly") => {
            // VALKEY_URL and REDIS_URL are still read, for older `.env` files.
            let url = var("TESS_CACHE_URL")
                .or_else(|| var("VALKEY_URL"))
                .or_else(|| var("REDIS_URL"))
                .with_context(|| {
                    format!("TESS_CACHE_URL is required for the {kind} cache (redis://host:6379)")
                })?;
            let name = match kind {
                "valkey" => "valkey",
                "redis" => "redis",
                "keydb" => "keydb",
                _ => "dragonfly",
            };
            Arc::new(tess_cache::RedisStore::new(&url, name)?)
        }
        other => bail!(
            "unsupported TESS_CACHE `{other}` (none, memory, valkey, redis, keydb, dragonfly)"
        ),
    };
    let mut ttl = Duration::from_secs(parse_var("TESS_CACHE_TTL_SECS", 60)?);
    let max = media_url_ttl.min(id_ttl) / 2;
    if ttl > max {
        tracing::warn!(
            ttl = ttl.as_secs(),
            max = max.as_secs(),
            "TESS_CACHE_TTL_SECS capped: cached URLs and ids would expire"
        );
        ttl = max;
    }
    if ttl.is_zero() {
        bail!("TESS_CACHE_TTL_SECS must be at least 1");
    }
    let config = tess_cache::CacheConfig {
        ttl,
        prefix: var_or("TESS_CACHE_PREFIX", "tess:"),
    };
    Ok(Some(tess_cache::CacheService::new(store, config)))
}

/// `None` when `TESS_PLUGINS=false`.
pub fn plugin_limits() -> Result<Option<tess_plugins::PluginLimits>> {
    if !bool_var("TESS_PLUGINS", true)? {
        return Ok(None);
    }
    let d = tess_plugins::PluginLimits::default();
    Ok(Some(tess_plugins::PluginLimits {
        max_module_bytes: parse_var("TESS_PLUGIN_MAX_MODULE_BYTES", d.max_module_bytes)?,
        max_timeout_ms: parse_var("TESS_PLUGIN_MAX_TIMEOUT_MS", d.max_timeout_ms)?,
        max_memory_pages: parse_var("TESS_PLUGIN_MAX_MEMORY_PAGES", d.max_memory_pages)?,
        allow_http: bool_var("TESS_PLUGIN_ALLOW_HTTP", d.allow_http)?,
    }))
}

/// Key sealing credentials stored in the database (webhook secrets, storage
/// keys of translation targets): `TESS_SECRETS_KEY`, else derived from
/// `TESS_SESSION_SECRET` (rotating that one then makes stored secrets unreadable).
pub fn secrets_key() -> Result<tess_core::secrets::SecretBox> {
    let (name, secret) = match var("TESS_SECRETS_KEY") {
        Some(s) => ("TESS_SECRETS_KEY", s),
        None => {
            tracing::warn!(
                "TESS_SECRETS_KEY is not set: stored integration secrets are sealed with TESS_SESSION_SECRET"
            );
            (
                "TESS_SESSION_SECRET",
                var("TESS_SESSION_SECRET").context("TESS_SESSION_SECRET is required")?,
            )
        }
    };
    tess_core::secrets::SecretBox::new(secret.as_bytes())
        .with_context(|| format!("{name} must be at least 32 bytes"))
}

fn list_var(name: &str) -> Vec<String> {
    var(name)
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// `None` when `TESS_WEBHOOKS=false`.
pub fn webhook_config() -> Result<Option<tess_webhooks::WebhookConfig>> {
    if !bool_var("TESS_WEBHOOKS", true)? {
        return Ok(None);
    }
    let d = tess_webhooks::WebhookConfig::default();
    Ok(Some(tess_webhooks::WebhookConfig {
        max_attempts: parse_var("TESS_WEBHOOK_MAX_ATTEMPTS", d.max_attempts)?.max(1),
        timeout: Duration::from_secs(parse_var("TESS_WEBHOOK_TIMEOUT_SECS", d.timeout.as_secs())?),
        // Lambda freezes the process once the response is sent.
        inline: running_on_lambda(),
        allowed_hosts: list_var("TESS_WEBHOOK_ALLOWED_HOSTS"),
        ..d
    }))
}

/// `None` when `TESS_I18N=false`.
pub fn i18n_config() -> Result<Option<tess_i18n::I18nConfig>> {
    if !bool_var("TESS_I18N", true)? {
        return Ok(None);
    }
    Ok(Some(tess_i18n::I18nConfig {
        inline: running_on_lambda(),
        ..Default::default()
    }))
}

/// `TESS_LICENSE`: `community` (default) or `commercial`. The commercial
/// license needs `TESS_LICENSE_KEY`; for now only its `tess_` prefix is
/// checked. The key is never logged.
pub fn license() -> Result<tess_graphql::License> {
    match var_or("TESS_LICENSE", "community")
        .to_ascii_lowercase()
        .as_str()
    {
        "community" => Ok(tess_graphql::License::Community),
        "commercial" => {
            let key = var("TESS_LICENSE_KEY")
                .context("TESS_LICENSE_KEY is required with TESS_LICENSE=commercial")?;
            if !key.starts_with("tess_") || key.len() <= "tess_".len() {
                bail!("TESS_LICENSE_KEY is not a Tess license key (it starts with `tess_`)");
            }
            Ok(tess_graphql::License::Commercial)
        }
        other => bail!("unsupported TESS_LICENSE `{other}` (community, commercial)"),
    }
}

/// Embedded n8n: `TESS_N8N_URL` is where Tess reaches it (e.g.
/// `http://n8n:5678`). Tess serves it under `TESS_N8N_PATH` (default `/n8n/`)
/// and strips that prefix; n8n must run with the same `N8N_PATH` so the
/// URLs it generates (assets, webhooks) carry it.
#[derive(Debug, Clone)]
pub struct N8nConfig {
    pub upstream: String,
    /// `/n8n`, without trailing slash.
    pub path: String,
}

pub fn n8n_config(license: tess_graphql::License) -> Result<Option<N8nConfig>> {
    let Some(upstream) = var("TESS_N8N_URL") else {
        return Ok(None);
    };
    if license == tess_graphql::License::Commercial {
        tracing::warn!(
            "TESS_N8N_URL is ignored: the commercial license doesn't include the n8n workflow editor"
        );
        return Ok(None);
    }
    if running_on_lambda() {
        bail!(
            "TESS_N8N_URL: the n8n proxy is not available on AWS Lambda; link to n8n directly instead"
        );
    }
    let upstream = upstream.trim_end_matches('/').to_string();
    if !(upstream.starts_with("http://") || upstream.starts_with("https://")) {
        bail!("TESS_N8N_URL must be an http(s) URL");
    }
    let path = format!("/{}", var_or("TESS_N8N_PATH", "/n8n/").trim_matches('/'));
    let reserved = [
        "/graphql", "/admin", "/api", "/media", "/i18n", "/mcp", "/health",
    ];
    if path == "/" || reserved.iter().any(|r| path == *r) {
        bail!("TESS_N8N_PATH `{path}` collides with a Tess route");
    }
    Ok(Some(N8nConfig { upstream, path }))
}
