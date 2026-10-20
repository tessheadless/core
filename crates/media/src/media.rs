// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use serde_json::{Map, Value, json};
use tess_core::{
    Actor, Cms, CmsError, ContentType, ContentTypeDef, Entry, EntryAction, EntryEvent, FieldDef,
    FieldType, Hook, Result,
};
use uuid::Uuid;

use crate::blob::{BlobStore, PresignedRequest};

/// apiId of the built-in content type describing uploaded files. Other
/// content types reference media with a relation field targeting it.
pub const ASSET_TYPE: &str = "Asset";

pub fn asset_definition() -> ContentTypeDef {
    let f = |name: &str, field_type: FieldType, required: bool| FieldDef {
        name: name.into(),
        field_type,
        required,
        description: None,
    };
    ContentTypeDef {
        workflow: false,
        api_id: ASSET_TYPE.into(),
        plural_api_id: Some("assets".into()),
        description: Some("Uploaded file (built-in).".into()),
        fields: vec![
            f("fileName", FieldType::text(), true),
            f("mimeType", FieldType::text(), true),
            f("size", FieldType::Int, false),
            f("key", FieldType::text(), true),
            f(
                "status",
                FieldType::Enum {
                    values: vec!["PENDING".into(), "READY".into()],
                },
                true,
            ),
            f("alt", FieldType::text(), false),
            f("width", FieldType::Int, false),
            f("height", FieldType::Int, false),
        ],
    }
}

#[derive(Debug, Clone)]
pub struct MediaConfig {
    /// Public base URL of the bucket or CDN (e.g. `https://cdn.example.com`).
    /// When set, asset URLs are `{base}/{key}`; otherwise presigned or proxied.
    pub public_base_url: Option<String>,
    /// Lifetime of presigned upload/download URLs.
    pub url_ttl: Duration,
    pub max_upload_bytes: u64,
    /// Path prefix the server proxies files under, for stores without presigning.
    pub proxy_path: String,
}

impl Default for MediaConfig {
    fn default() -> Self {
        MediaConfig {
            public_base_url: None,
            url_ttl: Duration::from_secs(3600),
            max_upload_bytes: 100 * 1024 * 1024,
            proxy_path: "/media/files".into(),
        }
    }
}

#[derive(Clone)]
pub struct MediaService {
    blobs: Arc<dyn BlobStore>,
    config: MediaConfig,
}

/// `assets/{128 random bits}/{sanitized file name}`. Unguessable, so proxied
/// URLs can be served without further authorization.
fn new_key(file_name: &str) -> String {
    let mut rnd = [0u8; 16];
    getrandom::fill(&mut rnd).expect("OS randomness");
    let hex: String = rnd.iter().map(|b| format!("{b:02x}")).collect();
    let mut name: String = file_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    name = name
        .trim_matches(|c| c == '.' || c == '-')
        .chars()
        .take(100)
        .collect();
    if name.is_empty() {
        name = "file".into();
    }
    format!("assets/{hex}/{name}")
}

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

impl MediaService {
    pub fn new(blobs: Arc<dyn BlobStore>, config: MediaConfig) -> Self {
        MediaService { blobs, config }
    }

    pub fn blobs(&self) -> &Arc<dyn BlobStore> {
        &self.blobs
    }

    pub fn config(&self) -> &MediaConfig {
        &self.config
    }

    /// Creates the `Asset` content type if missing, and registers the hook
    /// removing files when their asset is deleted.
    pub async fn install(&self, cms: &Cms) -> Result<()> {
        match cms.content_type(ASSET_TYPE).await {
            Ok(_) => {}
            Err(CmsError::NotFound(_)) => {
                cms.create_content_type(asset_definition(), &Actor::system())
                    .await?;
            }
            Err(e) => return Err(e),
        }
        cms.hooks().replace_prefixed(
            "builtin:media",
            vec![Arc::new(MediaHook {
                blobs: self.blobs.clone(),
            })],
        );
        Ok(())
    }

    fn check_size(&self, size: u64) -> Result<()> {
        if size > self.config.max_upload_bytes {
            return Err(CmsError::validation(format!(
                "file too large ({size} bytes, max {})",
                self.config.max_upload_bytes
            )));
        }
        Ok(())
    }

    /// Stores a file received by the server and creates its (draft) asset.
    pub async fn upload(
        &self,
        cms: &Cms,
        actor: &Actor,
        file_name: &str,
        mime_type: &str,
        data: Bytes,
    ) -> Result<Entry> {
        actor.require_edit(&cms.content_type(ASSET_TYPE).await?)?;
        self.check_size(data.len() as u64)?;
        let key = new_key(file_name);
        let size = data.len() as i64;
        self.blobs.put(&key, data, mime_type).await?;
        let entry = cms
            .create_entry(
                ASSET_TYPE,
                obj(json!({"fileName": file_name, "mimeType": mime_type, "size": size, "key": key, "status": "READY"})),
                actor,
            )
            .await;
        if entry.is_err() {
            let _ = self.blobs.delete(&key).await;
        }
        entry
    }

    /// Creates a `PENDING` asset and a presigned upload request for it; call
    /// [`Self::confirm_upload`] once the client has uploaded the file.
    pub async fn create_upload(
        &self,
        cms: &Cms,
        actor: &Actor,
        file_name: &str,
        mime_type: &str,
        size: u64,
    ) -> Result<(Entry, PresignedRequest)> {
        actor.require_edit(&cms.content_type(ASSET_TYPE).await?)?;
        self.check_size(size)?;
        let key = new_key(file_name);
        let Some(request) = self
            .blobs
            .presign_put(&key, mime_type, self.config.url_ttl)
            .await?
        else {
            return Err(CmsError::validation(format!(
                "the `{}` media store does not support direct uploads; POST the file to /media/upload",
                self.blobs.name()
            )));
        };
        let entry = cms
            .create_entry(
                ASSET_TYPE,
                obj(json!({"fileName": file_name, "mimeType": mime_type, "size": size, "key": key, "status": "PENDING"})),
                actor,
            )
            .await?;
        Ok((entry, request))
    }

    /// Marks a pending asset as ready after checking the object exists.
    pub async fn confirm_upload(&self, cms: &Cms, actor: &Actor, id: Uuid) -> Result<Entry> {
        let asset = cms
            .get_entry(ASSET_TYPE, id, tess_core::Stage::Draft)
            .await?
            .ok_or_else(|| CmsError::NotFound(format!("Asset `{id}`")))?;
        let key = asset
            .draft
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(info) = self.blobs.head(key).await? else {
            return Err(CmsError::validation("the file has not been uploaded yet"));
        };
        if let Err(e) = self.check_size(info.size) {
            let _ = self.blobs.delete(key).await;
            return Err(e);
        }
        cms.update_entry(
            ASSET_TYPE,
            id,
            obj(json!({"size": info.size as i64, "status": "READY"})),
            actor,
        )
        .await
    }

    /// Where clients download a file from.
    pub async fn url(&self, key: &str) -> Result<String> {
        if let Some(base) = &self.config.public_base_url {
            return Ok(format!("{}/{key}", base.trim_end_matches('/')));
        }
        if let Some(url) = self.blobs.presign_get(key, self.config.url_ttl).await? {
            return Ok(url);
        }
        Ok(format!("{}/{key}", self.config.proxy_path))
    }
}

/// Deletes the stored file when its asset entry is deleted.
pub struct MediaHook {
    blobs: Arc<dyn BlobStore>,
}

#[async_trait]
impl Hook for MediaHook {
    fn name(&self) -> &str {
        "builtin:media"
    }

    async fn after_write(&self, event: &EntryEvent) {
        if event.action != EntryAction::Delete || event.content_type != ASSET_TYPE {
            return;
        }
        let key = event
            .previous
            .as_ref()
            .and_then(|e| e.draft.get("key"))
            .and_then(Value::as_str);
        if let Some(key) = key
            && let Err(e) = self.blobs.delete(key).await
        {
            tracing::warn!(key, error = %e, "could not delete media file");
        }
    }

    async fn after_content_type_change(&self, _ct: &ContentType, _deleted: bool) {}
}

#[cfg(test)]
mod tests {
    use tess_core::memory::MemoryStorage;

    use super::*;
    use crate::FsBlobStore;

    #[tokio::test]
    async fn upload_and_delete_with_filesystem_store() {
        let dir = tempfile::tempdir().unwrap();
        let cms = Cms::new(Arc::new(MemoryStorage::new()));
        let media = MediaService::new(
            Arc::new(FsBlobStore::new(dir.path())),
            MediaConfig::default(),
        );
        media.install(&cms).await.unwrap();
        media.install(&cms).await.unwrap(); // idempotent

        let asset = media
            .upload(
                &cms,
                &Actor::system(),
                "My photo (1).JPG",
                "image/jpeg",
                Bytes::from_static(b"jpeg"),
            )
            .await
            .unwrap();
        let key = asset.draft["key"].as_str().unwrap().to_string();
        assert!(
            key.starts_with("assets/") && key.ends_with("/My-photo--1-.JPG"),
            "{key}"
        );
        assert_eq!(
            media.blobs().get(&key).await.unwrap().unwrap(),
            Bytes::from_static(b"jpeg")
        );
        assert_eq!(
            media.url(&key).await.unwrap(),
            format!("/media/files/{key}")
        );
        // Direct uploads need a presigning store.
        assert!(
            media
                .create_upload(&cms, &Actor::system(), "a.txt", "text/plain", 1)
                .await
                .is_err()
        );

        cms.delete_entry(ASSET_TYPE, asset.id, &Actor::system())
            .await
            .unwrap();
        assert!(media.blobs().get(&key).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let cms = Cms::new(Arc::new(MemoryStorage::new()));
        let config = MediaConfig {
            max_upload_bytes: 3,
            ..MediaConfig::default()
        };
        let media = MediaService::new(Arc::new(FsBlobStore::new(dir.path())), config);
        media.install(&cms).await.unwrap();
        let res = media
            .upload(
                &cms,
                &Actor::system(),
                "a",
                "text/plain",
                Bytes::from_static(b"four"),
            )
            .await;
        assert!(matches!(res, Err(CmsError::Validation(_))));
    }
}
