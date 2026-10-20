// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Outgoing webhooks, managed by admins at runtime.
//!
//! Every entry change (`entry.create`, `entry.publish`, ...), content model
//! change (`contentType.*`, `component.*`) and service event (`i18n.*`) is
//! POSTed as JSON to the webhooks subscribed to it, typically an n8n workflow
//! started by the *Tess Trigger* node:
//!
//! ```json
//! { "id": "<delivery id>", "event": "entry.publish", "createdAt": "2026-…Z",
//!   "contentType": "Post", "actor": { "name": "Ada" },
//!   "data": { "entry": { "id", "ref", "draft", "published", … }, "previous": { … } } }
//! ```
//!
//! Requests are signed: `X-Tess-Signature: t=<unix seconds>,v1=<hex
//! HMAC-SHA256(secret, "<t>.<body>")>`. Deliveries run in the background with
//! retries (network errors, 408, 429 and 5xx), and the outcome of the last
//! ones is kept per webhook. Signing secrets and header values are encrypted
//! at rest; PASSWORD fields are never sent.

mod hook;
mod service;

pub use hook::WebhookHook;
pub use service::{
    DeliveryOutcome, IdFn, WebhookConfig, WebhookInput, WebhookService, WebhookUpdate,
};

use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

/// Persisted webhook (settings key `webhooks/{id}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookRecord {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    /// Subscribed events: exact names (`entry.publish`), prefixes
    /// (`entry.*`) or `*` for everything.
    pub events: Vec<String>,
    /// Content types the webhook applies to; empty means all. Events without
    /// a content type (components, translations) are not filtered.
    pub content_types: Vec<String>,
    /// Extra request headers; values sealed with the secrets key.
    #[serde(default)]
    pub headers: Vec<SealedHeader>,
    /// Signing secret, sealed with the secrets key.
    pub secret: String,
    /// Who registered the webhook automatically (e.g. `n8n`), if anyone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_by: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SealedHeader {
    pub name: String,
    pub value: String,
}

impl WebhookRecord {
    pub fn subscribes_to(&self, event: &str, content_type: Option<&str>) -> bool {
        let event_ok = self.events.iter().any(|p| event_matches(p, event));
        let type_ok = match content_type {
            Some(ct) => self.content_types.is_empty() || self.content_types.iter().any(|c| c == ct),
            None => true,
        };
        self.enabled && event_ok && type_ok
    }
}

/// `*`, `prefix.*` or an exact event name.
pub fn event_matches(pattern: &str, event: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some("") => true,
        Some(prefix) => event.starts_with(prefix),
        None => pattern == event,
    }
}

/// Outcome of one delivery, as kept in the log (`webhook-log/{id}/…`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryRecord {
    pub id: String,
    pub webhook_id: String,
    pub event: String,
    pub status: DeliveryStatus,
    pub attempts: u32,
    /// HTTP status of the last attempt, if a response arrived.
    pub response_status: Option<u16>,
    /// Start of the body of the last response, or the network error.
    pub error: Option<String>,
    pub duration_ms: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeliveryStatus {
    /// Failed so far, another attempt is scheduled.
    Retrying,
    Success,
    Failed,
}

impl DeliveryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            DeliveryStatus::Retrying => "RETRYING",
            DeliveryStatus::Success => "SUCCESS",
            DeliveryStatus::Failed => "FAILED",
        }
    }
}

/// Value of the `X-Tess-Signature` header for `body` sent at `timestamp`.
pub fn signature(secret: &str, timestamp: i64, body: &[u8]) -> String {
    let mut mac =
        <Hmac<Sha256>>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    format!(
        "t={timestamp},v1={}",
        hex::encode(mac.finalize().into_bytes())
    )
}

/// Checks an `X-Tess-Signature` header (receivers written in Rust, tests).
pub fn verify_signature(secret: &str, header: &str, body: &[u8]) -> bool {
    let Some(t) = header
        .split(',')
        .find_map(|p| p.strip_prefix("t="))
        .and_then(|t| t.parse::<i64>().ok())
    else {
        return false;
    };
    let expected = signature(secret, t, body);
    let given = header.split(',').find_map(|p| p.strip_prefix("v1="));
    let wanted = expected.split(',').find_map(|p| p.strip_prefix("v1="));
    match (given, wanted) {
        (Some(g), Some(w)) => {
            g.len() == w.len()
                && g.bytes()
                    .zip(w.bytes())
                    .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                    == 0
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        assert!(event_matches("*", "entry.create"));
        assert!(event_matches("entry.*", "entry.publish"));
        assert!(!event_matches("entry.*", "contentType.create"));
        assert!(event_matches("contentType.delete", "contentType.delete"));
        assert!(!event_matches("contentType.delete", "contentType.update"));
    }

    #[test]
    fn signatures() {
        let s = signature("k", 1700000000, b"{}");
        assert!(s.starts_with("t=1700000000,v1="));
        assert!(verify_signature("k", &s, b"{}"));
        assert!(!verify_signature("k", &s, b"{ }"));
        assert!(!verify_signature("other", &s, b"{}"));
        assert!(!verify_signature("k", "garbage", b"{}"));
    }
}
