// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Argon2id password hashing (user accounts and PASSWORD fields), run on the
//! blocking pool because it is deliberately slow.

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};

use crate::error::{CmsError, Result};

pub async fn hash(password: String) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        Argon2::default().hash_password(password.as_bytes()).map(|h| h.to_string()).map_err(CmsError::storage)
    })
    .await
    .map_err(CmsError::storage)?
}

/// `false` for a wrong password or a malformed hash.
pub async fn verify(password: String, phc: String) -> Result<bool> {
    tokio::task::spawn_blocking(move || {
        let Ok(parsed) = PasswordHash::new(&phc) else { return Ok(false) };
        Ok(Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
    })
    .await
    .map_err(CmsError::storage)?
}
