// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::fmt::Display;

#[derive(Debug, thiserror::Error)]
pub enum CmsError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("conflict: {0}")]
    Conflict(String),
    /// The caller is authenticated but not allowed to do this.
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("storage error: {0}")]
    Storage(String),
    /// A plugin crashed, timed out or returned garbage.
    #[error("plugin error: {0}")]
    Plugin(String),
}

impl CmsError {
    pub fn storage(err: impl Display) -> Self {
        CmsError::Storage(err.to_string())
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        CmsError::Validation(msg.into())
    }
}

pub type Result<T, E = CmsError> = std::result::Result<T, E>;
