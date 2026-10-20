// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tess_core::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInfo {
    pub size: u64,
    pub content_type: Option<String>,
}

/// A request the client performs itself, directly against the object store.
#[derive(Debug, Clone)]
pub struct PresignedRequest {
    pub url: String,
    pub method: String,
    /// Headers the client must send unchanged (they are part of the signature).
    pub headers: Vec<(String, String)>,
    pub expires_in: Duration,
}

/// Storage for file contents. Keys are relative `/`-separated paths made of
/// `[A-Za-z0-9._-]` segments.
#[async_trait]
pub trait BlobStore: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    async fn put(&self, key: &str, data: Bytes, content_type: &str) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Option<Bytes>>;
    async fn head(&self, key: &str) -> Result<Option<ObjectInfo>>;
    /// Deleting a missing object is not an error.
    async fn delete(&self, key: &str) -> Result<()>;

    /// Presigned upload, letting clients bypass the CMS for large files (and
    /// the 6 MB request limit of AWS Lambda). `None` when unsupported.
    async fn presign_put(
        &self,
        _key: &str,
        _content_type: &str,
        _ttl: Duration,
    ) -> Result<Option<PresignedRequest>> {
        Ok(None)
    }

    /// Presigned download URL. `None` when unsupported.
    async fn presign_get(&self, _key: &str, _ttl: Duration) -> Result<Option<String>> {
        Ok(None)
    }
}

/// Rejects keys that could escape the storage root or need escaping.
pub(crate) fn check_key(key: &str) -> Result<()> {
    let ok = !key.is_empty()
        && key.len() <= 512
        && key.split('/').all(|seg| {
            !seg.is_empty()
                && seg != "."
                && seg != ".."
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        });
    if ok {
        Ok(())
    } else {
        Err(tess_core::CmsError::validation(format!(
            "invalid object key `{key}`"
        )))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn keys() {
        assert!(super::check_key("assets/ab12/photo.jpg").is_ok());
        for bad in ["", "/abs", "a/../b", "a//b", "a/./b", "sp ace", "a\\b", "é"] {
            assert!(super::check_key(bad).is_err(), "{bad}");
        }
    }
}
