// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Public form endpoints, for visitors (no authentication):
//!
//! - `GET /api/forms/{slug}`: the definition, to render the form;
//! - `POST /api/forms/{slug}`: a submission, as JSON or as a plain HTML form
//!   (`application/x-www-form-urlencoded`).

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde_json::{Map, Value, json};
use tess_forms::{FormsService, SubmitError, SubmitMeta};

use crate::AppState;

fn forms(state: &AppState) -> Option<&Arc<FormsService>> {
    state.service.services().forms.as_ref()
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "no such form" })),
    )
        .into_response()
}

pub async fn get_form(State(state): State<Arc<AppState>>, Path(slug): Path<String>) -> Response {
    let Some(svc) = forms(&state) else {
        return not_found();
    };
    match svc.public(&slug).await {
        Ok(Some(form)) => {
            let mut res = Json(form.public_json()).into_response();
            res.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=60"),
            );
            res
        }
        Ok(None) => not_found(),
        Err(e) => {
            tracing::error!(error = %e, "cannot read form");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

/// The visitor's address, for the rate limit. `X-Forwarded-For` is trusted
/// only from a local or private address (a reverse proxy in front of Tess),
/// or without a connection address (AWS Lambda).
fn client(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    let forwarded = || {
        headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let behind_proxy = |ip: IpAddr| match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unique_local(),
    };
    match peer {
        Some(addr) if behind_proxy(addr.ip()) => {
            forwarded().unwrap_or_else(|| addr.ip().to_string())
        }
        Some(addr) => addr.ip().to_string(),
        None => forwarded().unwrap_or_else(|| "unknown".into()),
    }
}

/// `a=1&b=x&b=y` as `{"a": "1", "b": ["x", "y"]}`.
fn form_pairs(body: &[u8]) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in form_urlencoded::parse(body) {
        let v = Value::String(v.into_owned());
        match out.get_mut(k.as_ref()) {
            Some(Value::Array(items)) => items.push(v),
            Some(first) => *first = Value::Array(vec![first.take(), v]),
            None => {
                out.insert(k.into_owned(), v);
            }
        }
    }
    out
}

pub async fn submit_form(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(svc) = forms(&state) else {
        return not_found();
    };
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let html_form = content_type.starts_with("application/x-www-form-urlencoded");
    let raw = if html_form {
        form_pairs(&body)
    } else if content_type.starts_with("application/json") {
        match serde_json::from_slice::<Value>(&body) {
            Ok(Value::Object(m)) => m,
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "the body must be a JSON object" })),
                )
                    .into_response();
            }
        }
    } else {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({ "error": "send application/json or application/x-www-form-urlencoded" })),
        )
            .into_response();
    };
    let meta = SubmitMeta {
        client: client(&headers, peer.map(|Extension(ConnectInfo(addr))| addr)),
        source: headers
            .get(header::REFERER)
            .and_then(|v| v.to_str().ok())
            .map(String::from),
    };
    match svc.submit(&slug, &raw, meta).await {
        Ok((form, submission)) => {
            if html_form && let Some(url) = &form.redirect_url {
                let mut res = StatusCode::SEE_OTHER.into_response();
                if let Ok(v) = HeaderValue::from_str(url) {
                    res.headers_mut().insert(header::LOCATION, v);
                }
                return res;
            }
            // A caught bot gets the same answer, with an id that leads nowhere.
            let id = submission.map_or_else(uuid::Uuid::new_v4, |s| s.id);
            (
                StatusCode::CREATED,
                Json(json!({ "ok": true, "id": id, "message": form.success_message })),
            )
                .into_response()
        }
        Err(SubmitError::NotFound) => not_found(),
        Err(SubmitError::Invalid(fields)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "some fields are invalid", "fields": fields })),
        )
            .into_response(),
        Err(SubmitError::RateLimited) => {
            let mut res = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": "too many submissions, try again in a minute" })),
            )
                .into_response();
            res.headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
            res
        }
        Err(SubmitError::Storage(e)) => {
            tracing::error!(error = %e, "cannot store a form submission");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencoded_repeated_keys_become_lists() {
        let m = form_pairs(b"name=Ada+L&extras=Cake&extras=Wine&terms=on");
        assert_eq!(
            Value::Object(m),
            json!({"name": "Ada L", "extras": ["Cake", "Wine"], "terms": "on"})
        );
    }

    #[test]
    fn forwarded_for_only_from_a_proxy() {
        let mut h = HeaderMap::new();
        h.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.9, 10.0.0.2"),
        );
        assert_eq!(
            client(&h, Some("10.0.0.2:5000".parse().unwrap())),
            "203.0.113.9"
        );
        assert_eq!(
            client(&h, Some("198.51.100.4:5000".parse().unwrap())),
            "198.51.100.4",
            "spoofed header ignored"
        );
        assert_eq!(
            client(&h, None),
            "203.0.113.9",
            "Lambda: no connection address"
        );
    }
}
