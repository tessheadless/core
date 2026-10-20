// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! REST endpoints for binary payloads, which do not fit GraphQL well:
//! media uploads/downloads and plugin module uploads.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde_json::{Value, json};
use tess_core::query::{EntryQuery, Filter, Op};
use tess_core::{Actor, CmsError, Stage};
use tess_graphql::Principal;
use tess_media::ASSET_TYPE;

use crate::AppState;

fn error(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (status, Json(json!({"error": message.into(), "code": code}))).into_response()
}

fn cms_error(e: CmsError) -> Response {
    match e {
        CmsError::NotFound(m) => error(StatusCode::NOT_FOUND, "NOT_FOUND", m),
        CmsError::Validation(m) => error(StatusCode::BAD_REQUEST, "BAD_USER_INPUT", m),
        CmsError::Conflict(m) => error(StatusCode::CONFLICT, "CONFLICT", m),
        CmsError::Forbidden(m) => error(StatusCode::FORBIDDEN, "FORBIDDEN", m),
        CmsError::Plugin(m) => error(StatusCode::UNPROCESSABLE_ENTITY, "PLUGIN_ERROR", m),
        CmsError::Storage(m) => {
            tracing::error!(error = %m, "storage failure");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_SERVER_ERROR",
                "internal storage error",
            )
        }
    }
}

/// The signed-in actor; role checks happen in the services.
async fn require_actor(state: &AppState, headers: &HeaderMap) -> Result<Actor, Box<Response>> {
    match state.principal(headers).await {
        Ok(Principal::User(actor)) => Ok(actor),
        Ok(_) => Err(Box::new(error(
            StatusCode::UNAUTHORIZED,
            "UNAUTHENTICATED",
            "sign in required",
        ))),
        Err(status) => Err(Box::new(error(
            status,
            "UNAUTHENTICATED",
            "invalid credentials",
        ))),
    }
}

#[derive(serde::Deserialize)]
pub struct UploadParams {
    #[serde(rename = "fileName")]
    file_name: String,
}

/// `POST /media/upload?fileName=photo.jpg` with the raw file as body and its
/// MIME type as `Content-Type`. Works with every media store.
pub async fn upload_media(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<UploadParams>,
    body: Bytes,
) -> Response {
    let actor = match require_actor(&state, &headers).await {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let Some(media) = &state.service.services().media else {
        return error(StatusCode::NOT_FOUND, "NOT_FOUND", "media is disabled");
    };
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .unwrap_or("application/octet-stream")
        .to_string();
    match media
        .upload(state.service.cms(), &actor, &params.file_name, &mime, body)
        .await
    {
        Ok(entry) => {
            let key = entry
                .draft
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let url = media.url(&key).await.unwrap_or_default();
            let codec = state.service.codec();
            let mut out = entry.draft.clone();
            out.insert("id".into(), json!(codec.encode(ASSET_TYPE, entry.id)));
            out.insert("ref".into(), json!(codec.stable_ref(ASSET_TYPE, entry.id)));
            out.insert("url".into(), json!(url));
            (StatusCode::CREATED, Json(Value::Object(out))).into_response()
        }
        Err(e) => cms_error(e),
    }
}

/// Only plain, well-formed MIME types are echoed back.
fn safe_mime(m: &str) -> &str {
    let ok = m.len() < 128
        && m.split_once('/').is_some_and(|(a, b)| {
            [a, b].iter().all(|p| {
                !p.is_empty()
                    && p.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"+-.".contains(&c))
            })
        });
    if ok { m } else { "application/octet-stream" }
}

/// `GET /media/files/{key}`: serves files when the store has no public or
/// presigned URLs (filesystem store). Keys contain 128 random bits.
pub async fn serve_media(State(state): State<Arc<AppState>>, Path(key): Path<String>) -> Response {
    let Some(media) = &state.service.services().media else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut q = EntryQuery::new(Stage::Draft);
    q.limit = 1;
    q.filter = Some(Filter::cmp("key", Op::Eq, key.clone()));
    let asset = match state.service.cms().query_entries(ASSET_TYPE, &q).await {
        Ok(page) => page.items.into_iter().next(),
        Err(e) => return cms_error(e),
    };
    let Some(asset) = asset else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let data = match media.blobs().get(&key).await {
        Ok(Some(d)) => d,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => return cms_error(e),
    };
    let mime = safe_mime(
        asset
            .draft
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
    .to_string();
    let mut res = data.into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&mime)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // User uploads (SVG, HTML) must never run scripts in our origin.
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    res
}

fn plugin_json(status: &tess_plugins::PluginStatus) -> Value {
    let mut v = serde_json::to_value(&status.record).unwrap_or_default();
    if let Value::Object(m) = &mut v {
        m.remove("moduleKey");
        m.insert("loaded".into(), json!(status.loaded));
        m.insert("lastError".into(), json!(status.last_error));
    }
    v
}

/// `POST /api/plugins/{name}` with the `.wasm` module as body.
pub async fn upload_plugin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    body: Bytes,
) -> Response {
    let actor = match require_actor(&state, &headers).await {
        Ok(a) => a,
        Err(r) => return *r,
    };
    if let Err(e) = actor.require_admin() {
        return cms_error(e);
    }
    let Some(plugins) = &state.service.services().plugins else {
        return error(StatusCode::NOT_FOUND, "NOT_FOUND", "plugins are disabled");
    };
    if let Err(e) = plugins.install(&name, body).await {
        return cms_error(e);
    }
    if let Err(e) = plugins.refresh(state.service.cms()).await {
        return cms_error(e);
    }
    match plugins.get(&name).await {
        Ok(status) => (StatusCode::CREATED, Json(plugin_json(&status))).into_response(),
        Err(e) => cms_error(e),
    }
}
