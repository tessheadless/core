// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::Client;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use bytes::Bytes;
use tess_core::{CmsError, Result};

use crate::blob::{BlobStore, ObjectInfo, PresignedRequest, check_key};

#[derive(Debug, Clone)]
pub struct S3Config {
    pub bucket: String,
    /// Custom endpoint for S3-compatible services (Garage, MinIO, R2, ...).
    pub endpoint: Option<String>,
    /// Region; Garage uses `garage` by default.
    pub region: Option<String>,
    /// Path-style URLs (`endpoint/bucket/key`), required by most self-hosted services.
    pub force_path_style: bool,
    /// Explicit credentials; when `None`, the default AWS chain is used (env,
    /// profile, IAM role of the Lambda function / ECS task, ...).
    pub credentials: Option<(String, String)>,
}

pub struct S3BlobStore {
    client: Client,
    bucket: String,
}

fn err(e: impl std::error::Error) -> CmsError {
    let mut msg = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        source = s.source();
    }
    CmsError::Storage(msg)
}

impl S3BlobStore {
    pub async fn connect(config: S3Config) -> Self {
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
        if let Some(region) = config.region {
            loader = loader.region(aws_config::Region::new(region));
        }
        if let Some((id, secret)) = config.credentials {
            loader = loader.credentials_provider(aws_sdk_s3::config::Credentials::new(id, secret, None, None, "tess-config"));
        }
        let shared = loader.load().await;
        let mut builder = aws_sdk_s3::config::Builder::from(&shared).force_path_style(config.force_path_style);
        if let Some(endpoint) = config.endpoint {
            builder = builder.endpoint_url(endpoint);
        }
        S3BlobStore { client: Client::from_conf(builder.build()), bucket: config.bucket }
    }

    /// Creates the bucket if missing (local development).
    pub async fn ensure_bucket(&self) -> Result<()> {
        if self.client.head_bucket().bucket(&self.bucket).send().await.is_ok() {
            return Ok(());
        }
        self.client.create_bucket().bucket(&self.bucket).send().await.map_err(err)?;
        Ok(())
    }
}

#[async_trait]
impl BlobStore for S3BlobStore {
    fn name(&self) -> &'static str {
        "s3"
    }

    async fn put(&self, key: &str, data: Bytes, content_type: &str) -> Result<()> {
        check_key(key)?;
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(err)?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Bytes>> {
        check_key(key)?;
        match self.client.get_object().bucket(&self.bucket).key(key).send().await {
            Ok(out) => Ok(Some(out.body.collect().await.map_err(err)?.into_bytes())),
            Err(e) if e.as_service_error().is_some_and(|s| s.is_no_such_key()) => Ok(None),
            Err(e) => Err(err(e)),
        }
    }

    async fn head(&self, key: &str) -> Result<Option<ObjectInfo>> {
        check_key(key)?;
        match self.client.head_object().bucket(&self.bucket).key(key).send().await {
            Ok(out) => Ok(Some(ObjectInfo {
                size: out.content_length.unwrap_or(0).max(0) as u64,
                content_type: out.content_type,
            })),
            Err(e) if e.as_service_error().is_some_and(|s| s.is_not_found()) => Ok(None),
            Err(e) => Err(err(e)),
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        check_key(key)?;
        self.client.delete_object().bucket(&self.bucket).key(key).send().await.map_err(err)?;
        Ok(())
    }

    async fn presign_put(&self, key: &str, content_type: &str, ttl: Duration) -> Result<Option<PresignedRequest>> {
        check_key(key)?;
        let config = PresigningConfig::expires_in(ttl).map_err(err)?;
        let req = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .presigned(config)
            .await
            .map_err(err)?;
        Ok(Some(PresignedRequest {
            url: req.uri().to_string(),
            method: req.method().to_string(),
            headers: req.headers().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            expires_in: ttl,
        }))
    }

    async fn presign_get(&self, key: &str, ttl: Duration) -> Result<Option<String>> {
        check_key(key)?;
        let config = PresigningConfig::expires_in(ttl).map_err(err)?;
        let req = self.client.get_object().bucket(&self.bucket).key(key).presigned(config).await.map_err(err)?;
        Ok(Some(req.uri().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs against an S3-compatible service (e.g. Garage) configured through
    /// `TESS_TEST_S3_ENDPOINT`, `TESS_TEST_S3_BUCKET`, `TESS_TEST_S3_KEY_ID`,
    /// `TESS_TEST_S3_SECRET`; skipped when unset.
    #[tokio::test]
    async fn s3_roundtrip_and_presigning() {
        let Ok(endpoint) = std::env::var("TESS_TEST_S3_ENDPOINT") else {
            eprintln!("TESS_TEST_S3_ENDPOINT not set, skipping");
            return;
        };
        let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} must be set"));
        let store = S3BlobStore::connect(S3Config {
            bucket: env("TESS_TEST_S3_BUCKET"),
            endpoint: Some(endpoint),
            region: Some(std::env::var("TESS_TEST_S3_REGION").unwrap_or_else(|_| "garage".into())),
            force_path_style: true,
            credentials: Some((env("TESS_TEST_S3_KEY_ID"), env("TESS_TEST_S3_SECRET"))),
        })
        .await;
        let key = "tests/roundtrip.txt";
        store.put(key, Bytes::from_static(b"hello"), "text/plain").await.unwrap();
        assert_eq!(store.get(key).await.unwrap().unwrap(), Bytes::from_static(b"hello"));
        let info = store.head(key).await.unwrap().unwrap();
        assert_eq!(info.size, 5);
        assert_eq!(info.content_type.as_deref(), Some("text/plain"));

        let get_url = store.presign_get(key, Duration::from_secs(60)).await.unwrap().unwrap();
        assert!(get_url.contains("X-Amz-Signature"), "{get_url}");
        let put = store.presign_put("tests/presigned.txt", "text/plain", Duration::from_secs(60)).await.unwrap().unwrap();
        assert_eq!(put.method, "PUT");

        store.delete(key).await.unwrap();
        store.delete(key).await.unwrap();
        assert!(store.get(key).await.unwrap().is_none());
        assert!(store.head(key).await.unwrap().is_none());
    }
}
