// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Account passwords: Argon2id, shared with PASSWORD fields (`tess_core::password`).

use tess_core::Result;

pub(crate) use tess_core::password::{hash, verify};

/// Spends the same time as a verification, for unknown accounts.
pub(crate) async fn burn(password: String) {
    let _: Result<String> = hash(password).await;
}
