// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Opaque, expiring object IDs for the GraphQL API (in the spirit of Rails'
//! GlobalID), plus a stable per-object `ref`.
//!
//! Storage IDs are UUIDs and never leave the server:
//!
//! - **`id`** = `base64url(version | key_id | nonce | XChaCha20-Poly1305(type, uuid, exp))`.
//!   It is *encrypted*, not just signed, so the UUID cannot be read by
//!   clients; the random nonce makes it different on every response; `exp`
//!   makes it unusable after the configured TTL. `key_id` lets previous keys
//!   keep decoding during a rotation.
//! - **`ref`** = `base64url(HMAC-SHA256(ref_key, type | uuid))[..16 bytes]`.
//!   Deterministic, so clients can tell two responses describe the same
//!   object, and one-way, so it does not reveal the UUID. It is meant for
//!   comparison/cache keys only, not for lookups.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const VERSION: u8 = 1;
const NONCE_LEN: usize = 24;
const REF_LEN: usize = 16;
/// Minimum secret length, in bytes.
pub const MIN_SECRET_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error("malformed id")]
    Malformed,
    /// Bad authentication tag, unknown key, or tampered token.
    #[error("invalid id")]
    Invalid,
    #[error("id has expired, refetch the object to get a fresh one")]
    Expired,
    #[error("id refers to a `{found}`, expected a `{expected}`")]
    WrongType { expected: String, found: String },
}

#[derive(Debug, thiserror::Error)]
pub enum IdConfigError {
    #[error("{0} must be at least {MIN_SECRET_LEN} bytes")]
    SecretTooShort(&'static str),
    #[error("TESS_ID_SECRET and TESS_REF_SECRET must differ")]
    SameSecrets,
    #[error("invalid duration `{0}` (expected e.g. `900s`, `30m`, `24h`, `7d`)")]
    BadTtl(String),
    #[error("TTL must be greater than zero")]
    ZeroTtl,
}

/// Secrets are supplied at runtime (environment / secrets manager), never
/// hard-coded.
pub struct IdConfig {
    /// Key used to encrypt new IDs (`TESS_ID_SECRET`).
    pub secret: Vec<u8>,
    /// Older keys still accepted for decoding (`TESS_ID_SECRET_PREVIOUS`).
    pub previous_secrets: Vec<Vec<u8>>,
    /// Key for stable refs (`TESS_REF_SECRET`). Rotating it changes every ref.
    pub ref_secret: Vec<u8>,
    /// Lifetime of issued IDs (`TESS_ID_TTL`).
    pub ttl: Duration,
}

struct EncKey {
    id: u8,
    cipher: XChaCha20Poly1305,
}

pub struct IdCodec {
    keys: Vec<EncKey>,
    ref_key: [u8; 32],
    ttl: Duration,
}

/// Derives a purpose-specific 32-byte key so one secret is never used for two
/// algorithms.
fn derive(secret: &[u8], purpose: &str) -> [u8; 32] {
    let mut mac = <Hmac<Sha256>>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(purpose.as_bytes());
    mac.finalize().into_bytes().into()
}

impl IdCodec {
    pub fn new(config: IdConfig) -> Result<Self, IdConfigError> {
        if config.secret.len() < MIN_SECRET_LEN {
            return Err(IdConfigError::SecretTooShort("TESS_ID_SECRET"));
        }
        if config.ref_secret.len() < MIN_SECRET_LEN {
            return Err(IdConfigError::SecretTooShort("TESS_REF_SECRET"));
        }
        if config
            .previous_secrets
            .iter()
            .any(|s| s.len() < MIN_SECRET_LEN)
        {
            return Err(IdConfigError::SecretTooShort("TESS_ID_SECRET_PREVIOUS"));
        }
        if config.secret == config.ref_secret {
            return Err(IdConfigError::SameSecrets);
        }
        if config.ttl.is_zero() {
            return Err(IdConfigError::ZeroTtl);
        }
        let keys = std::iter::once(&config.secret)
            .chain(&config.previous_secrets)
            .map(|secret| {
                let key = derive(secret, "tess/id/enc/v1");
                EncKey {
                    // Public, non-secret fingerprint used to pick the key.
                    id: Sha256::digest(key)[0],
                    cipher: XChaCha20Poly1305::new(&key.into()),
                }
            })
            .collect();
        Ok(IdCodec {
            keys,
            ref_key: derive(&config.ref_secret, "tess/ref/v1"),
            ttl: config.ttl,
        })
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// Issues a fresh opaque ID for an object of GraphQL type `type_name`.
    pub fn encode(&self, type_name: &str, id: Uuid) -> String {
        self.encode_at(type_name, id, unix_now())
    }

    pub fn encode_at(&self, type_name: &str, id: Uuid, now: i64) -> String {
        let key = &self.keys[0];
        let exp = now.saturating_add(self.ttl.as_secs() as i64);
        let type_bytes = type_name.as_bytes();
        assert!(
            type_bytes.len() <= u8::MAX as usize,
            "type names are at most 64 chars"
        );

        let mut plain = Vec::with_capacity(1 + type_bytes.len() + 16 + 8);
        plain.push(type_bytes.len() as u8);
        plain.extend_from_slice(type_bytes);
        plain.extend_from_slice(id.as_bytes());
        plain.extend_from_slice(&exp.to_be_bytes());

        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).expect("OS random number generator");
        let header = [VERSION, key.id];
        let sealed = key
            .cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &plain,
                    aad: &header,
                },
            )
            .expect("encryption with a valid key cannot fail");

        let mut token = Vec::with_capacity(header.len() + NONCE_LEN + sealed.len());
        token.extend_from_slice(&header);
        token.extend_from_slice(&nonce);
        token.extend_from_slice(&sealed);
        URL_SAFE_NO_PAD.encode(token)
    }

    /// Decodes an opaque ID. With `expected_type`, IDs of other types are
    /// rejected (an `Author` id cannot be used as a `Post` id).
    pub fn decode(
        &self,
        token: &str,
        expected_type: Option<&str>,
    ) -> Result<(String, Uuid), IdError> {
        self.decode_at(token, expected_type, unix_now())
    }

    pub fn decode_at(
        &self,
        token: &str,
        expected_type: Option<&str>,
        now: i64,
    ) -> Result<(String, Uuid), IdError> {
        // Reject absurd inputs before decoding.
        if token.len() > 512 {
            return Err(IdError::Malformed);
        }
        let raw = URL_SAFE_NO_PAD
            .decode(token)
            .map_err(|_| IdError::Malformed)?;
        if raw.len() < 2 + NONCE_LEN + 16 || raw[0] != VERSION {
            return Err(IdError::Malformed);
        }
        let (header, rest) = raw.split_at(2);
        let (nonce, sealed) = rest.split_at(NONCE_LEN);
        let nonce = XNonce::try_from(nonce).map_err(|_| IdError::Malformed)?;
        let plain = self
            .keys
            .iter()
            .filter(|k| k.id == header[1])
            .find_map(|k| {
                k.cipher
                    .decrypt(
                        &nonce,
                        Payload {
                            msg: sealed,
                            aad: header,
                        },
                    )
                    .ok()
            })
            .ok_or(IdError::Invalid)?;

        // Authenticated, so the layout below is ours; still parse defensively.
        let type_len = *plain.first().ok_or(IdError::Invalid)? as usize;
        if plain.len() != 1 + type_len + 16 + 8 {
            return Err(IdError::Invalid);
        }
        let type_name =
            std::str::from_utf8(&plain[1..1 + type_len]).map_err(|_| IdError::Invalid)?;
        let id = Uuid::from_slice(&plain[1 + type_len..1 + type_len + 16])
            .map_err(|_| IdError::Invalid)?;
        let exp = i64::from_be_bytes(
            plain[1 + type_len + 16..]
                .try_into()
                .map_err(|_| IdError::Invalid)?,
        );

        if now >= exp {
            return Err(IdError::Expired);
        }
        if let Some(expected) = expected_type
            && expected != type_name
        {
            return Err(IdError::WrongType {
                expected: expected.into(),
                found: type_name.into(),
            });
        }
        Ok((type_name.to_string(), id))
    }

    /// Stable, non-reversible reference for an object: identical across
    /// requests, different for every object.
    pub fn stable_ref(&self, type_name: &str, id: Uuid) -> String {
        let mut mac =
            <Hmac<Sha256>>::new_from_slice(&self.ref_key).expect("HMAC accepts any key length");
        mac.update(&[type_name.len() as u8]);
        mac.update(type_name.as_bytes());
        mac.update(id.as_bytes());
        URL_SAFE_NO_PAD.encode(&mac.finalize().into_bytes()[..REF_LEN])
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parses `900`, `900s`, `30m`, `24h`, `7d`.
pub fn parse_ttl(s: &str) -> Result<Duration, IdConfigError> {
    let s = s.trim();
    let bad = || IdConfigError::BadTtl(s.to_string());
    let (num, unit) = match s.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        Some((i, _)) => s.split_at(i),
        None => (s, "s"),
    };
    let n: u64 = num.parse().map_err(|_| bad())?;
    let mult = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => return Err(bad()),
    };
    let secs = n.checked_mul(mult).ok_or_else(bad)?;
    if secs == 0 {
        return Err(IdConfigError::ZeroTtl);
    }
    Ok(Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Random per-test secrets: no key material lives in the repository.
    fn secret() -> Vec<u8> {
        let mut s = vec![0u8; 32];
        getrandom::fill(&mut s).unwrap();
        s
    }

    fn codec_with(secret_bytes: Vec<u8>, previous: Vec<Vec<u8>>, ref_secret: Vec<u8>) -> IdCodec {
        IdCodec::new(IdConfig {
            secret: secret_bytes,
            previous_secrets: previous,
            ref_secret,
            ttl: Duration::from_secs(3600),
        })
        .unwrap()
    }

    fn codec() -> IdCodec {
        codec_with(secret(), vec![], secret())
    }

    #[test]
    fn round_trip_and_uuid_not_visible() {
        let c = codec();
        let id = Uuid::new_v4();
        let token = c.encode("Post", id);
        assert_eq!(
            c.decode(&token, Some("Post")).unwrap(),
            ("Post".to_string(), id)
        );
        assert!(!token.contains(&id.simple().to_string()));
        let raw = URL_SAFE_NO_PAD.decode(&token).unwrap();
        assert!(
            !raw.windows(16).any(|w| w == id.as_bytes()),
            "uuid bytes must not appear in clear"
        );
    }

    #[test]
    fn ids_change_on_every_encoding_but_ref_is_stable() {
        let c = codec();
        let id = Uuid::new_v4();
        assert_ne!(c.encode("Post", id), c.encode("Post", id));
        assert_eq!(c.stable_ref("Post", id), c.stable_ref("Post", id));
        assert_ne!(
            c.stable_ref("Post", id),
            c.stable_ref("Post", Uuid::new_v4())
        );
        assert_ne!(c.stable_ref("Post", id), c.stable_ref("Author", id));
        assert!(!c.stable_ref("Post", id).contains(&id.simple().to_string()));
    }

    #[test]
    fn expiry() {
        let c = codec();
        let id = Uuid::new_v4();
        let token = c.encode_at("Post", id, 1_000);
        assert!(c.decode_at(&token, None, 1_000 + 3599).is_ok());
        assert_eq!(
            c.decode_at(&token, None, 1_000 + 3600),
            Err(IdError::Expired)
        );
    }

    #[test]
    fn wrong_type_rejected() {
        let c = codec();
        let token = c.encode("Author", Uuid::new_v4());
        assert_eq!(
            c.decode(&token, Some("Post")),
            Err(IdError::WrongType {
                expected: "Post".into(),
                found: "Author".into()
            })
        );
    }

    #[test]
    fn tampering_and_garbage_rejected() {
        let c = codec();
        let token = c.encode("Post", Uuid::new_v4());
        let mut raw = URL_SAFE_NO_PAD.decode(&token).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 1;
        assert_eq!(
            c.decode(&URL_SAFE_NO_PAD.encode(&raw), None),
            Err(IdError::Invalid)
        );
        // Header is authenticated too.
        let mut raw = URL_SAFE_NO_PAD.decode(&token).unwrap();
        raw[1] ^= 0xff;
        assert_eq!(
            c.decode(&URL_SAFE_NO_PAD.encode(&raw), None),
            Err(IdError::Invalid)
        );
        assert_eq!(c.decode("not-an-id!", None), Err(IdError::Malformed));
        assert_eq!(
            c.decode(&Uuid::new_v4().to_string(), None),
            Err(IdError::Malformed)
        );
        assert_eq!(c.decode("", None), Err(IdError::Malformed));
    }

    #[test]
    fn other_key_rejected_and_rotation_accepted() {
        let (old, new, refs) = (secret(), secret(), secret());
        let before = codec_with(old.clone(), vec![], refs.clone());
        let id = Uuid::new_v4();
        let token = before.encode("Post", id);

        let unrelated = codec();
        assert_eq!(unrelated.decode(&token, None), Err(IdError::Invalid));

        let rotated = codec_with(new, vec![old], refs);
        assert_eq!(rotated.decode(&token, None).unwrap().1, id);
        // Refs survive an ID-key rotation.
        assert_eq!(
            rotated.stable_ref("Post", id),
            before.stable_ref("Post", id)
        );
    }

    #[test]
    fn config_validation() {
        let short = vec![1u8; 8];
        let ok = secret();
        let cfg = |s: Vec<u8>, r: Vec<u8>, ttl| IdConfig {
            secret: s,
            previous_secrets: vec![],
            ref_secret: r,
            ttl,
        };
        assert!(IdCodec::new(cfg(short.clone(), secret(), Duration::from_secs(1))).is_err());
        assert!(IdCodec::new(cfg(secret(), short, Duration::from_secs(1))).is_err());
        assert!(matches!(
            IdCodec::new(cfg(ok.clone(), ok, Duration::from_secs(1))),
            Err(IdConfigError::SameSecrets)
        ));
        assert!(IdCodec::new(cfg(secret(), secret(), Duration::ZERO)).is_err());
    }

    #[test]
    fn ttl_parsing() {
        assert_eq!(parse_ttl("900").unwrap(), Duration::from_secs(900));
        assert_eq!(parse_ttl("30m").unwrap(), Duration::from_secs(1800));
        assert_eq!(parse_ttl("24h").unwrap(), Duration::from_secs(86_400));
        assert_eq!(parse_ttl("7d").unwrap(), Duration::from_secs(604_800));
        assert!(parse_ttl("0s").is_err());
        assert!(parse_ttl("1w").is_err());
        assert!(parse_ttl("h").is_err());
    }
}
