// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! HTTP routes of the built-in integrations:
//!
//! - `GET /i18n/{format}/{file}`: live translation files (the layout of
//!   `exportTranslations`, e.g. `/i18n/json/fr.json`, `/i18n/i18next/fr/common.json`).
//! - `GET /api/i18n/export?format=…`: download an export (zipped when it has several files).
//! - `/n8n/*`: reverse proxy to the embedded n8n instance (prefix stripped),
//!   so the admin UI can show the n8n editor in a same-origin iframe.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::TryStreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tess_graphql::Principal;
use tess_i18n::{ExportOptions, Format, TranslationState};

use crate::AppState;
use crate::config::N8nConfig;

fn i18n(state: &AppState) -> Option<&Arc<tess_i18n::I18nService>> {
    state.service.services().i18n.as_ref()
}

fn status_of(e: &tess_core::CmsError) -> StatusCode {
    match e {
        tess_core::CmsError::NotFound(_) => StatusCode::NOT_FOUND,
        tess_core::CmsError::Validation(_) => StatusCode::BAD_REQUEST,
        tess_core::CmsError::Forbidden(_) => StatusCode::FORBIDDEN,
        tess_core::CmsError::Conflict(_) => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn error(e: tess_core::CmsError) -> Response {
    let status = status_of(&e);
    let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
        tracing::error!(error = %e, "i18n failure");
        "internal error".to_string()
    } else {
        e.to_string()
    };
    (status, axum::Json(serde_json::json!({ "error": message }))).into_response()
}

/// `GET /i18n/{format}/{*path}`.
pub async fn serve_translations(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((format, path)): Path<(String, String)>,
) -> Response {
    let Some(svc) = i18n(&state) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(format) = Format::parse(&format) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let settings = match svc.settings().await {
        Ok(s) => s,
        Err(e) => return error(e),
    };
    let public = match state.principal(&headers).await {
        Ok(Principal::User(_)) => false,
        Ok(Principal::Public) if settings.public => true,
        Ok(_) => return (StatusCode::UNAUTHORIZED, "sign in required").into_response(),
        Err(status) => return status.into_response(),
    };
    let file = match served_file(&state, svc, format, &path).await {
        Ok(Some(f)) => f,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => return error(e),
    };
    let etag = format!("\"{}\"", hex_prefix(file.content.as_bytes()));
    let cache = if public {
        "public, max-age=60"
    } else {
        "private, no-cache"
    };
    let mut res = if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        file.content.into_response()
    };
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&file.content_type).unwrap_or(HeaderValue::from_static("text/plain")),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(v) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, v);
    }
    res
}

/// A rendered translation file, from the response cache when enabled
/// (rendering every key in every format is the costly part).
async fn served_file(
    state: &AppState,
    svc: &tess_i18n::I18nService,
    format: Format,
    path: &str,
) -> tess_core::Result<Option<tess_i18n::ExportFile>> {
    let Some(cache) = &state.cache else {
        return svc.serve(format, path).await;
    };
    let lookup = cache
        .lookup(
            tess_cache::Scope::Translations,
            &[format.slug().as_bytes(), path.as_bytes()],
        )
        .await;
    if let Some(file) = lookup
        .value
        .and_then(|v| serde_json::from_slice::<serde_json::Value>(&v).ok())
        .and_then(|v| {
            Some(tess_i18n::ExportFile {
                path: path.to_string(),
                content: v["content"].as_str()?.to_string(),
                content_type: v["contentType"].as_str()?.to_string(),
            })
        })
    {
        return Ok(Some(file));
    }
    let file = svc.serve(format, path).await?;
    if let (Some(f), Some(key)) = (&file, lookup.key) {
        let v = serde_json::json!({ "content": f.content, "contentType": f.content_type });
        cache.store(&key, v.to_string().as_bytes()).await;
    }
    Ok(file)
}

fn hex_prefix(data: &[u8]) -> String {
    Sha256::digest(data)[..12]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportParams {
    format: String,
    languages: Option<String>,
    namespaces: Option<String>,
    states: Option<String>,
    fallback_to_base: Option<bool>,
}

fn csv(v: &Option<String>) -> Vec<String> {
    v.as_deref()
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// `GET /api/i18n/export?format=JSON&languages=en,fr&namespaces=&states=REVIEWED&fallbackToBase=true`.
pub async fn download_export(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ExportParams>,
) -> Response {
    match state.principal(&headers).await {
        Ok(Principal::User(_)) => {}
        Ok(_) => return (StatusCode::UNAUTHORIZED, "sign in required").into_response(),
        Err(status) => return status.into_response(),
    }
    let Some(svc) = i18n(&state) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(format) = Format::parse(&params.format) else {
        return (StatusCode::BAD_REQUEST, "unknown format").into_response();
    };
    let states: Vec<TranslationState> = csv(&params.states)
        .iter()
        .filter_map(|s| TranslationState::parse(s))
        .collect();
    let opts = ExportOptions {
        format,
        languages: csv(&params.languages),
        namespaces: csv(&params.namespaces),
        states: if states.is_empty() {
            TranslationState::ALL.to_vec()
        } else {
            states
        },
        fallback_to_base: params.fallback_to_base.unwrap_or(false),
    };
    let files = match svc.export(&opts).await {
        Ok(f) => f,
        Err(e) => return error(e),
    };
    let (body, content_type, name) = if let [file] = files.as_slice() {
        let name = file
            .path
            .rsplit('/')
            .next()
            .unwrap_or("translations")
            .to_string();
        (
            file.content.clone().into_bytes(),
            file.content_type.clone(),
            name,
        )
    } else {
        match tess_i18n::zip_files(&files) {
            Ok(zip) => (
                zip,
                "application/zip".to_string(),
                format!("translations-{}.zip", format.slug()),
            ),
            Err(e) => return error(e),
        }
    };
    let disposition = format!("attachment; filename=\"{}\"", name.replace('"', ""));
    let mut res = body.into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    if let Ok(v) = HeaderValue::from_str(&disposition) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    res
}

// ---- n8n reverse proxy -----------------------------------------------------

pub struct N8nProxy {
    config: N8nConfig,
    client: reqwest::Client,
}

impl N8nProxy {
    pub fn new(config: N8nConfig) -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("HTTP client");
        N8nProxy { config, client }
    }

    pub fn path(&self) -> &str {
        &self.config.path
    }
}

/// Connection-level headers, never forwarded (RFC 9110 §7.6.1).
fn hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
    )
}

/// n8n serves everything at its root (`N8N_PATH` only changes the URLs it
/// generates), so the prefix is removed: `/n8n/rest/x?y` → `/rest/x?y`.
fn upstream_path(prefix: &str, uri: &axum::http::Uri) -> String {
    let path = uri.path().strip_prefix(prefix).unwrap_or(uri.path());
    let path = if path.is_empty() { "/" } else { path };
    match uri.query() {
        Some(q) => format!("{path}?{q}"),
        None => path.to_string(),
    }
}

/// Responses whose content is written by workflow authors (webhook replies,
/// n8n forms). They must not run with the Tess origin: a page there could
/// read the admin's credentials.
fn user_content(n8n_path: &str, path: &str) -> bool {
    let rest = path.strip_prefix(n8n_path).unwrap_or(path);
    [
        "/webhook",
        "/webhook-test",
        "/webhook-waiting",
        "/form",
        "/form-test",
        "/form-waiting",
    ]
    .iter()
    .any(|p| rest == *p || rest.starts_with(&format!("{p}/")))
}

pub async fn proxy_n8n(State(state): State<Arc<AppState>>, req: Request) -> Response {
    let Some(proxy) = &state.n8n else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (parts, body) = req.into_parts();
    let url = format!(
        "{}{}",
        proxy.config.upstream,
        upstream_path(&proxy.config.path, &parts.uri)
    );
    let mut upstream = proxy.client.request(parts.method.clone(), url);
    for (name, value) in &parts.headers {
        if !hop_by_hop(name) && !name.as_str().starts_with("x-forwarded-") {
            upstream = upstream.header(name, value);
        }
    }
    if let Some(host) = parts.headers.get(header::HOST) {
        upstream = upstream.header("x-forwarded-host", host);
    }
    let proto = parts
        .headers
        .get("x-forwarded-proto")
        .cloned()
        .unwrap_or(HeaderValue::from_static("http"));
    upstream = upstream.header("x-forwarded-proto", proto);
    if let Some(ip) = parts
        .extensions
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
    {
        upstream = upstream.header("x-forwarded-for", ip.0.ip().to_string());
    }
    let stream = body.into_data_stream();
    let response = match upstream
        .body(reqwest::Body::wrap_stream(stream))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "n8n is unreachable");
            return (
                StatusCode::BAD_GATEWAY,
                "The workflow engine (n8n) is not reachable.",
            )
                .into_response();
        }
    };
    let status = response.status();
    let mut headers = HeaderMap::new();
    for (name, value) in response.headers() {
        if !hop_by_hop(name) {
            headers.append(name.clone(), value.clone());
        }
    }
    if user_content(&proxy.config.path, parts.uri.path()) {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "sandbox allow-scripts allow-forms allow-popups allow-downloads",
            ),
        );
    }
    // Streamed, so server-sent events (n8n push) flow through.
    let body = Body::from_stream(response.bytes_stream().map_err(std::io::Error::other));
    let mut res = Response::new(body);
    *res.status_mut() = status;
    *res.headers_mut() = headers;
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_content_paths() {
        assert!(user_content("/n8n", "/n8n/webhook/abc"));
        assert!(user_content("/n8n", "/n8n/form/x"));
        assert!(user_content("/n8n", "/n8n/webhook-test/abc"));
        assert!(!user_content("/n8n", "/n8n/"));
        assert!(!user_content("/n8n", "/n8n/rest/workflows"));
        assert!(!user_content("/n8n", "/n8n/webhooks-editor"));
        let uri = |s: &str| s.parse::<axum::http::Uri>().unwrap();
        assert_eq!(
            upstream_path("/n8n", &uri("/n8n/rest/login?x=1")),
            "/rest/login?x=1"
        );
        assert_eq!(upstream_path("/n8n", &uri("/n8n")), "/");
        assert_eq!(upstream_path("/n8n", &uri("/n8n/")), "/");
        assert!(hop_by_hop(&header::CONNECTION));
        assert!(!hop_by_hop(&header::COOKIE));
    }
}
