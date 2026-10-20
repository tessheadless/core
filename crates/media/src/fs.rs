// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::path::PathBuf;

use async_trait::async_trait;
use bytes::Bytes;
use tess_core::{CmsError, Result};

use crate::blob::{BlobStore, ObjectInfo, check_key};

/// Files under a local directory (Docker volume). No presigned URLs: uploads
/// and downloads go through the CMS server.
pub struct FsBlobStore {
    root: PathBuf,
}

impl FsBlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FsBlobStore { root: root.into() }
    }

    fn path(&self, key: &str) -> Result<PathBuf> {
        check_key(key)?;
        Ok(self.root.join(key))
    }
}

#[async_trait]
impl BlobStore for FsBlobStore {
    fn name(&self) -> &'static str {
        "filesystem"
    }

    async fn put(&self, key: &str, data: Bytes, _content_type: &str) -> Result<()> {
        let path = self.path(key)?;
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(CmsError::storage)?;
        }
        // Write to a temp file then rename, so readers never see partial files.
        let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
        tokio::fs::write(&tmp, &data).await.map_err(CmsError::storage)?;
        tokio::fs::rename(&tmp, &path).await.map_err(CmsError::storage)
    }

    async fn get(&self, key: &str) -> Result<Option<Bytes>> {
        match tokio::fs::read(self.path(key)?).await {
            Ok(data) => Ok(Some(Bytes::from(data))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(CmsError::storage(e)),
        }
    }

    async fn head(&self, key: &str) -> Result<Option<ObjectInfo>> {
        match tokio::fs::metadata(self.path(key)?).await {
            Ok(m) if m.is_file() => Ok(Some(ObjectInfo { size: m.len(), content_type: None })),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(CmsError::storage(e)),
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        match tokio::fs::remove_file(self.path(key)?).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(CmsError::storage(e)),
            _ => Ok(()),
        }
    }
}
