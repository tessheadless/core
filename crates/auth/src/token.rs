// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Signed session tokens and API token secrets.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

const SESSION_PREFIX: &str = "tess_s1.";
const TOKEN_PREFIX: &str = "tess_t1_";

/// Key signing session tokens (from `TESS_SESSION_SECRET`).
pub struct SessionKey([u8; 32]);

impl SessionKey {
    /// Derives the signing key; the secret must have at least 32 bytes.
    pub fn new(secret: &[u8]) -> Option<Self> {
        if secret.len() < 32 {
            return None;
        }
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret).ok()?;
        mac.update(b"tess/session/v1");
        Some(SessionKey(mac.finalize().into_bytes().into()))
    }

    fn mac(&self, payload: &[u8]) -> Hmac<Sha256> {
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&self.0).expect("any key length");
        mac.update(payload);
        mac
    }

    pub(crate) fn sign(&self, user: Uuid, version: u32, expires_at: DateTime<Utc>) -> String {
        let claims = format!("{user}.{version}.{}", expires_at.timestamp());
        let payload = B64.encode(claims);
        let sig = B64.encode(self.mac(payload.as_bytes()).finalize().into_bytes());
        format!("{SESSION_PREFIX}{payload}.{sig}")
    }

    /// `(user, session_version, expires_at)` when the signature is valid.
    pub(crate) fn verify(&self, token: &str) -> Option<(Uuid, u32, DateTime<Utc>)> {
        let rest = token.strip_prefix(SESSION_PREFIX)?;
        let (payload, sig) = rest.split_once('.')?;
        let sig = B64.decode(sig).ok()?;
        self.mac(payload.as_bytes()).verify_slice(&sig).ok()?;
        let claims = String::from_utf8(B64.decode(payload).ok()?).ok()?;
        let mut parts = claims.split('.');
        let user = Uuid::parse_str(parts.next()?).ok()?;
        let version = parts.next()?.parse().ok()?;
        let expires_at = DateTime::from_timestamp(parts.next()?.parse().ok()?, 0)?;
        Some((user, version, expires_at))
    }
}

pub(crate) fn is_session(bearer: &str) -> bool {
    bearer.starts_with(SESSION_PREFIX)
}

fn sha256_hex(s: &str) -> String {
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn random(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf).expect("OS randomness");
    buf
}

/// `(id, full token string, stored hash)`.
pub(crate) fn new_api_token() -> (String, String, String) {
    let id: String = random(8).iter().map(|b| format!("{b:02x}")).collect();
    let secret = B64.encode(random(32));
    (id.clone(), format!("{TOKEN_PREFIX}{id}_{secret}"), sha256_hex(&secret))
}

/// `(id, secret)` from `tess_t1_<id>_<secret>`.
pub(crate) fn parse_api_token(bearer: &str) -> Option<(&str, &str)> {
    let rest = bearer.strip_prefix(TOKEN_PREFIX)?;
    let (id, secret) = rest.split_once('_')?;
    (id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit()) && !secret.is_empty()).then_some((id, secret))
}

pub(crate) fn secret_matches(secret: &str, stored_hash: &str) -> bool {
    sha256_hex(secret).as_bytes().ct_eq(stored_hash.as_bytes()).into()
}
