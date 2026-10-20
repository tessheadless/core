// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Encryption at rest for credentials that admins configure at runtime
//! (webhook signing secrets, storage keys of translation delivery targets).
//!
//! Values are sealed with XChaCha20-Poly1305 under a key derived from
//! `TESS_SECRETS_KEY`, bound to a purpose string (associated data) so a
//! ciphertext cannot be moved from one setting to another. Sealed values are
//! stored as `v1:<base64url(nonce || ciphertext)>` and never returned by the API.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

use crate::error::{CmsError, Result};

const PREFIX: &str = "v1:";
const NONCE_LEN: usize = 24;

#[derive(Clone)]
pub struct SecretBox {
    cipher: XChaCha20Poly1305,
}

impl std::fmt::Debug for SecretBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretBox(..)")
    }
}

impl SecretBox {
    /// `secret` must hold at least 32 bytes of entropy (e.g. `openssl rand -hex 32`).
    pub fn new(secret: &[u8]) -> Result<Self> {
        if secret.len() < 32 {
            return Err(CmsError::validation(
                "the secrets key must be at least 32 bytes",
            ));
        }
        use sha2::Digest;
        let key: [u8; 32] = sha2::Sha256::new_with_prefix(b"tess-secrets-v1\0")
            .chain_update(secret)
            .finalize()
            .into();
        Ok(SecretBox {
            cipher: XChaCha20Poly1305::new(&key.into()),
        })
    }

    pub fn seal(&self, purpose: &str, plain: &str) -> String {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).expect("OS random number generator");
        let sealed = self
            .cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: plain.as_bytes(),
                    aad: purpose.as_bytes(),
                },
            )
            .expect("encryption with a valid key cannot fail");
        let mut raw = nonce.to_vec();
        raw.extend(sealed);
        format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(raw))
    }

    pub fn open(&self, purpose: &str, sealed: &str) -> Result<String> {
        let bad = || {
            CmsError::Storage(format!(
                "cannot decrypt a stored secret ({purpose}): wrong TESS_SECRETS_KEY?"
            ))
        };
        let raw = sealed
            .strip_prefix(PREFIX)
            .and_then(|b| URL_SAFE_NO_PAD.decode(b).ok())
            .ok_or_else(bad)?;
        if raw.len() <= NONCE_LEN {
            return Err(bad());
        }
        let (nonce, msg) = raw.split_at(NONCE_LEN);
        let nonce = XNonce::try_from(nonce).map_err(|_| bad())?;
        let plain = self
            .cipher
            .decrypt(
                &nonce,
                Payload {
                    msg,
                    aad: purpose.as_bytes(),
                },
            )
            .map_err(|_| bad())?;
        String::from_utf8(plain).map_err(|_| bad())
    }
}

/// Random secret, 32 bytes as base64url (webhook signing secrets).
pub fn random_secret() -> String {
    let mut raw = [0u8; 32];
    getrandom::fill(&mut raw).expect("OS random number generator");
    URL_SAFE_NO_PAD.encode(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_purpose_binding() {
        let b = SecretBox::new(&[7u8; 32]).unwrap();
        let s = b.seal("webhook/1", "hunter2");
        assert!(s.starts_with("v1:"));
        assert_eq!(b.open("webhook/1", &s).unwrap(), "hunter2");
        assert!(b.open("webhook/2", &s).is_err());
        assert!(
            SecretBox::new(&[8u8; 32])
                .unwrap()
                .open("webhook/1", &s)
                .is_err()
        );
        assert!(b.open("webhook/1", "v1:garbage").is_err());
        assert!(SecretBox::new(b"short").is_err());
        assert_ne!(random_secret(), random_secret());
    }
}
