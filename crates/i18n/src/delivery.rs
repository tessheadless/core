// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Delivery targets: exported files published to object storage, where
//! apps and CDNs read them (Tolgee's “content delivery”).
//!
//! - `S3`: any S3-compatible service. Presets in the admin UI fill the
//!   endpoint for Garage, AWS S3, Cloudflare R2 (`https://<account>.r2.cloudflarestorage.com`,
//!   region `auto`) and MinIO.
//! - `AZURE_BLOB`: Azure Blob Storage, with the account key (Shared Key
//!   signing) or a SAS token (`sp=cw` on the container is enough).
//!
//! Files land at `{pathPrefix}/{format}/{file}` (the same layout as the
//! live `/i18n/{format}/{file}` URLs), next to a `manifest.json`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use bytes::Bytes;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use tess_core::model::{format_ts, now};
use tess_core::{Actor, CmsError, Result};
use uuid::Uuid;

use crate::formats::{ExportOptions, Format};
use crate::service::I18nService;
use crate::{TranslationState, normalize_tag};

const TARGETS_PREFIX: &str = "i18n/targets/";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StorageKind {
    S3,
    AzureBlob,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AzureAuth {
    #[default]
    SharedKey,
    Sas,
}

/// Persisted target (settings key `i18n/targets/{id}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryTarget {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub kind: StorageKind,
    /// Preset chosen in the UI (`garage`, `aws`, `r2`, `minio`, `azure`, `other`).
    pub provider: String,
    /// S3 bucket or Azure container.
    pub bucket: String,
    pub endpoint: Option<String>,
    pub region: Option<String>,
    #[serde(default)]
    pub force_path_style: bool,
    /// S3 access key id, or Azure storage account name.
    pub access_key_id: Option<String>,
    /// S3 secret key, Azure account key or SAS token, sealed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(default)]
    pub azure_auth: AzureAuth,
    #[serde(default)]
    pub path_prefix: String,
    /// Where the published files can be read (shown in the UI and manifest).
    pub public_url: Option<String>,
    pub formats: Vec<Format>,
    /// Empty means all.
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub namespaces: Vec<String>,
    pub states: Vec<TranslationState>,
    #[serde(default)]
    pub fallback_to_base: bool,
    /// Publish a few seconds after every translation change.
    pub auto_publish: bool,
    pub last_published_at: Option<String>,
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_file_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

impl DeliveryTarget {
    pub fn has_secret(&self) -> bool {
        self.secret.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct DeliveryInput {
    pub name: String,
    pub enabled: bool,
    pub kind: StorageKind,
    pub provider: String,
    pub bucket: String,
    pub endpoint: Option<String>,
    pub region: Option<String>,
    pub force_path_style: bool,
    pub access_key_id: Option<String>,
    pub secret: Option<String>,
    pub azure_auth: AzureAuth,
    pub path_prefix: String,
    pub public_url: Option<String>,
    pub formats: Vec<Format>,
    pub languages: Vec<String>,
    pub namespaces: Vec<String>,
    pub states: Vec<TranslationState>,
    pub fallback_to_base: bool,
    pub auto_publish: bool,
}

/// `None` keeps the current value; for options, `Some(None)` clears.
#[derive(Debug, Clone, Default)]
pub struct DeliveryUpdate {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub provider: Option<String>,
    pub bucket: Option<String>,
    pub endpoint: Option<Option<String>>,
    pub region: Option<Option<String>>,
    pub force_path_style: Option<bool>,
    pub access_key_id: Option<Option<String>>,
    pub secret: Option<Option<String>>,
    pub azure_auth: Option<AzureAuth>,
    pub path_prefix: Option<String>,
    pub public_url: Option<Option<String>>,
    pub formats: Option<Vec<Format>>,
    pub languages: Option<Vec<String>>,
    pub namespaces: Option<Vec<String>>,
    pub states: Option<Vec<TranslationState>>,
    pub fallback_to_base: Option<bool>,
    pub auto_publish: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishReport {
    pub target_id: String,
    pub name: String,
    pub ok: bool,
    pub files: u32,
    pub error: Option<String>,
    pub published_at: String,
}

fn key(id: &str) -> String {
    format!("{TARGETS_PREFIX}{id}")
}

fn purpose(id: &str) -> String {
    format!("i18n-target/{id}/secret")
}

fn blank(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn check_url(field: &str, url: &Option<String>) -> Result<()> {
    if let Some(u) = url {
        let parsed = reqwest::Url::parse(u)
            .map_err(|_| CmsError::validation(format!("{field}: `{u}` is not a valid URL")))?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err(CmsError::validation(format!(
                "{field} must be an http(s) URL"
            )));
        }
    }
    Ok(())
}

fn check_prefix(prefix: &str) -> Result<String> {
    let p = prefix.trim().trim_matches('/').to_string();
    let ok = p.is_empty()
        || p.split('/').all(|seg| {
            !seg.is_empty()
                && seg != "."
                && seg != ".."
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        });
    if ok {
        Ok(p)
    } else {
        Err(CmsError::validation(
            "the path prefix may only hold letters, digits, `.`, `_`, `-` and `/`",
        ))
    }
}

fn check(t: &DeliveryTarget) -> Result<()> {
    if t.name.trim().is_empty() || t.name.chars().count() > 100 {
        return Err(CmsError::validation(
            "the target name must have 1 to 100 characters",
        ));
    }
    if t.formats.is_empty() {
        return Err(CmsError::validation("choose at least one format"));
    }
    if t.states.is_empty() {
        return Err(CmsError::validation(
            "choose at least one translation state",
        ));
    }
    check_url("endpoint", &t.endpoint)?;
    check_url("publicUrl", &t.public_url)?;
    let bucket_ok = (3..=63).contains(&t.bucket.len())
        && t.bucket
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-.".contains(&b));
    if !bucket_ok {
        return Err(CmsError::validation(
            "bucket/container names have 3 to 63 lowercase letters, digits, `-` or `.`",
        ));
    }
    match t.kind {
        StorageKind::S3 => {
            if t.access_key_id.is_some() != t.secret.is_some() {
                return Err(CmsError::validation(
                    "give both the access key id and the secret key, or neither (default AWS credentials)",
                ));
            }
        }
        StorageKind::AzureBlob => {
            let account = t.access_key_id.as_deref().unwrap_or_default();
            if !(3..=24).contains(&account.len())
                || !account
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            {
                return Err(CmsError::validation(
                    "the Azure storage account name has 3 to 24 lowercase letters or digits",
                ));
            }
            if t.secret.is_none() {
                return Err(CmsError::validation(
                    "Azure targets need the account key or a SAS token",
                ));
            }
        }
    }
    Ok(())
}

/// Counter of translation changes, so a debounced publish only runs for the
/// last change of a burst.
static GENERATION: AtomicU64 = AtomicU64::new(0);

impl I18nService {
    pub async fn targets(&self) -> Result<Vec<DeliveryTarget>> {
        let mut all: Vec<DeliveryTarget> = self
            .storage
            .list_settings(TARGETS_PREFIX)
            .await?
            .into_iter()
            .map(|(_, v)| serde_json::from_value(v).map_err(CmsError::storage))
            .collect::<Result<_>>()?;
        all.sort_by_key(|t| t.name.to_lowercase());
        Ok(all)
    }

    pub async fn target(&self, id: &str) -> Result<DeliveryTarget> {
        let v = self
            .storage
            .get_setting(&key(id))
            .await?
            .ok_or_else(|| CmsError::NotFound(format!("target `{id}`")))?;
        serde_json::from_value(v).map_err(CmsError::storage)
    }

    async fn save_target(&self, t: &DeliveryTarget) -> Result<()> {
        let v = serde_json::to_value(t).map_err(CmsError::storage)?;
        self.storage.put_setting(&key(&t.id), &v).await
    }

    fn languages_of(&self, tags: Vec<String>) -> Result<Vec<String>> {
        tags.iter().map(|t| normalize_tag(t)).collect()
    }

    pub async fn create_target(
        &self,
        input: DeliveryInput,
        actor: &Actor,
    ) -> Result<DeliveryTarget> {
        actor.require_admin()?;
        let id = Uuid::new_v4().to_string();
        let ts = format_ts(now());
        let t = DeliveryTarget {
            secret: blank(input.secret).map(|s| self.secrets.seal(&purpose(&id), &s)),
            id,
            name: input.name.trim().to_string(),
            enabled: input.enabled,
            kind: input.kind,
            provider: input.provider,
            bucket: input.bucket.trim().to_string(),
            endpoint: blank(input.endpoint),
            region: blank(input.region),
            force_path_style: input.force_path_style,
            access_key_id: blank(input.access_key_id),
            azure_auth: input.azure_auth,
            path_prefix: check_prefix(&input.path_prefix)?,
            public_url: blank(input.public_url),
            formats: input.formats,
            languages: self.languages_of(input.languages)?,
            namespaces: input.namespaces,
            states: input.states,
            fallback_to_base: input.fallback_to_base,
            auto_publish: input.auto_publish,
            last_published_at: None,
            last_error: None,
            last_file_count: 0,
            created_at: ts.clone(),
            updated_at: ts,
        };
        check(&t)?;
        self.save_target(&t).await?;
        Ok(t)
    }

    pub async fn update_target(
        &self,
        id: &str,
        u: DeliveryUpdate,
        actor: &Actor,
    ) -> Result<DeliveryTarget> {
        actor.require_admin()?;
        let mut t = self.target(id).await?;
        macro_rules! set {
            ($field:ident) => {
                if let Some(v) = u.$field {
                    t.$field = v;
                }
            };
            ($field:ident, opt) => {
                if let Some(v) = u.$field {
                    t.$field = blank(v);
                }
            };
        }
        set!(name);
        set!(enabled);
        set!(provider);
        set!(bucket);
        set!(endpoint, opt);
        set!(region, opt);
        set!(force_path_style);
        set!(access_key_id, opt);
        set!(azure_auth);
        set!(public_url, opt);
        set!(formats);
        set!(namespaces);
        set!(states);
        set!(fallback_to_base);
        set!(auto_publish);
        if let Some(langs) = u.languages {
            t.languages = self.languages_of(langs)?;
        }
        if let Some(p) = u.path_prefix {
            t.path_prefix = check_prefix(&p)?;
        }
        if let Some(secret) = u.secret {
            t.secret = blank(secret).map(|s| self.secrets.seal(&purpose(id), &s));
        }
        t.name = t.name.trim().to_string();
        check(&t)?;
        t.updated_at = format_ts(now());
        self.save_target(&t).await?;
        Ok(t)
    }

    pub async fn delete_target(&self, id: &str, actor: &Actor) -> Result<()> {
        actor.require_admin()?;
        if !self.storage.delete_setting(&key(id)).await? {
            return Err(CmsError::NotFound(format!("target `{id}`")));
        }
        Ok(())
    }

    /// Publishes one target, or every enabled one when `id` is `None`.
    pub async fn publish(&self, id: Option<&str>, actor: &Actor) -> Result<Vec<PublishReport>> {
        actor.require_admin()?;
        let targets = match id {
            Some(id) => vec![self.target(id).await?],
            None => self
                .targets()
                .await?
                .into_iter()
                .filter(|t| t.enabled)
                .collect(),
        };
        let mut reports = vec![];
        for t in targets {
            reports.push(self.publish_target(t, actor).await);
        }
        Ok(reports)
    }

    async fn publish_target(&self, mut t: DeliveryTarget, actor: &Actor) -> PublishReport {
        let published_at = format_ts(now());
        let result = self.upload_all(&t, &published_at).await;
        let (ok, files, error) = match result {
            Ok(n) => (true, n, None),
            Err(e) => (false, 0, Some(e.to_string())),
        };
        if ok {
            t.last_published_at = Some(published_at.clone());
            t.last_file_count = files;
        }
        t.last_error = error.clone();
        // Re-read so concurrent settings changes are kept.
        if let Ok(mut fresh) = self.target(&t.id).await {
            fresh.last_published_at = t.last_published_at.clone();
            fresh.last_file_count = t.last_file_count;
            fresh.last_error = t.last_error.clone();
            if let Err(e) = self.save_target(&fresh).await {
                tracing::error!(error = %e, "cannot record a translation publish");
            }
        }
        let report = PublishReport {
            target_id: t.id.clone(),
            name: t.name.clone(),
            ok,
            files,
            error,
            published_at,
        };
        if !ok {
            tracing::warn!(target = %t.name, error = ?report.error, "translation publish failed");
        }
        self.emit("i18n.publish", json!(report), actor).await;
        report
    }

    async fn upload_all(&self, t: &DeliveryTarget, published_at: &str) -> Result<u32> {
        let uploader = self.uploader(t).await?;
        let prefix = if t.path_prefix.is_empty() {
            String::new()
        } else {
            format!("{}/", t.path_prefix)
        };
        let mut manifest: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        let mut count = 0;
        for format in &t.formats {
            let opts = ExportOptions {
                format: *format,
                languages: t.languages.clone(),
                namespaces: t.namespaces.clone(),
                states: t.states.clone(),
                fallback_to_base: t.fallback_to_base,
            };
            for file in self.export(&opts).await? {
                let path = format!("{}/{}", format.slug(), file.path);
                uploader
                    .put(
                        &format!("{prefix}{path}"),
                        Bytes::from(file.content),
                        &file.content_type,
                    )
                    .await?;
                manifest.entry(format.slug()).or_default().push(path);
                count += 1;
            }
        }
        let languages: Vec<String> = self.languages().await?.into_iter().map(|l| l.tag).collect();
        let body = json!({
            "publishedAt": published_at,
            "baseUrl": t.public_url,
            "languages": if t.languages.is_empty() { languages } else { t.languages.clone() },
            "files": manifest,
        });
        let body = serde_json::to_vec_pretty(&body).unwrap_or_default();
        uploader
            .put(
                &format!("{prefix}manifest.json"),
                Bytes::from(body),
                "application/json; charset=utf-8",
            )
            .await?;
        Ok(count + 1)
    }

    async fn uploader(&self, t: &DeliveryTarget) -> Result<Box<dyn Uploader>> {
        let secret = t
            .secret
            .as_ref()
            .map(|s| self.secrets.open(&purpose(&t.id), s))
            .transpose()?;
        match t.kind {
            #[cfg(feature = "s3")]
            StorageKind::S3 => {
                let store = tess_media::S3BlobStore::connect(tess_media::S3Config {
                    public_endpoint: None,
                    bucket: t.bucket.clone(),
                    endpoint: t.endpoint.clone(),
                    region: t.region.clone(),
                    force_path_style: t.force_path_style,
                    credentials: t.access_key_id.clone().zip(secret),
                })
                .await;
                Ok(Box::new(S3Uploader(store)))
            }
            #[cfg(not(feature = "s3"))]
            StorageKind::S3 => Err(CmsError::validation("this build has no S3 support")),
            StorageKind::AzureBlob => Ok(Box::new(AzureUploader {
                client: reqwest::Client::new(),
                account: t.access_key_id.clone().unwrap_or_default(),
                container: t.bucket.clone(),
                endpoint: t.endpoint.clone(),
                auth: t.azure_auth,
                secret: secret.unwrap_or_default(),
            })),
        }
    }

    /// Called after every translation change: schedules the auto-publish targets.
    pub(crate) async fn after_change(self: &Arc<Self>) {
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let Ok(targets) = self.targets().await else {
            return;
        };
        if !targets.iter().any(|t| t.enabled && t.auto_publish) {
            return;
        }
        if self.config.inline {
            self.publish_auto().await;
            return;
        }
        let this = self.clone();
        let delay = self.config.debounce;
        let handle = tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            if GENERATION.load(Ordering::SeqCst) == generation {
                this.publish_auto().await;
            }
        });
        *self.auto.lock().expect("auto lock") = Some(handle);
    }

    async fn publish_auto(&self) {
        let Ok(targets) = self.targets().await else {
            return;
        };
        for t in targets.into_iter().filter(|t| t.enabled && t.auto_publish) {
            self.publish_target(t, &Actor::system()).await;
        }
    }
}

#[async_trait]
trait Uploader: Send + Sync {
    async fn put(&self, key: &str, body: Bytes, content_type: &str) -> Result<()>;
}

#[cfg(feature = "s3")]
struct S3Uploader(tess_media::S3BlobStore);

#[cfg(feature = "s3")]
#[async_trait]
impl Uploader for S3Uploader {
    async fn put(&self, key: &str, body: Bytes, content_type: &str) -> Result<()> {
        use tess_media::BlobStore;
        self.0.put(key, body, content_type).await
    }
}

struct AzureUploader {
    client: reqwest::Client,
    account: String,
    container: String,
    /// Custom endpoint including the account path for Azurite
    /// (`http://127.0.0.1:10000/devstoreaccount1`).
    endpoint: Option<String>,
    auth: AzureAuth,
    secret: String,
}

const AZURE_VERSION: &str = "2021-08-06";

/// Percent-encodes a blob path (segments kept).
fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            seg.bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// `Authorization: SharedKey` for a Put Blob request.
pub(crate) fn azure_shared_key(
    account: &str,
    key_b64: &str,
    url_path: &str,
    content_length: usize,
    content_type: &str,
    ms_headers: &[(&str, &str)],
) -> Result<String> {
    let key = STANDARD
        .decode(key_b64.trim())
        .map_err(|_| CmsError::validation("the Azure account key is not valid base64"))?;
    let mut headers: Vec<(String, &str)> = ms_headers
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), *v))
        .collect();
    headers.sort();
    let canonical_headers: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let length = if content_length == 0 {
        String::new()
    } else {
        content_length.to_string()
    };
    // VERB, Content-Encoding, Content-Language, Content-Length, Content-MD5,
    // Content-Type, Date, If-Modified-Since, If-Match, If-None-Match,
    // If-Unmodified-Since, Range, then headers and resource.
    let to_sign = format!(
        "PUT\n\n\n{length}\n\n{content_type}\n\n\n\n\n\n\n{canonical_headers}/{account}{url_path}"
    );
    let mut mac = <Hmac<Sha256>>::new_from_slice(&key).expect("HMAC accepts any key length");
    mac.update(to_sign.as_bytes());
    Ok(format!(
        "SharedKey {account}:{}",
        STANDARD.encode(mac.finalize().into_bytes())
    ))
}

#[async_trait]
impl Uploader for AzureUploader {
    async fn put(&self, key: &str, body: Bytes, content_type: &str) -> Result<()> {
        let base = match &self.endpoint {
            Some(e) => e.trim_end_matches('/').to_string(),
            None => format!("https://{}.blob.core.windows.net", self.account),
        };
        let blob = format!("/{}/{}", self.container, encode_path(key));
        let mut url = format!("{base}{blob}");
        if self.auth == AzureAuth::Sas {
            url.push('?');
            url.push_str(self.secret.trim().trim_start_matches('?'));
        }
        let date = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let mut req = self
            .client
            .put(&url)
            .header("x-ms-blob-type", "BlockBlob")
            .header("x-ms-date", &date)
            .header("x-ms-version", AZURE_VERSION)
            .header("Content-Type", content_type);
        if self.auth == AzureAuth::SharedKey {
            // The canonical resource is the URL path, which for Azurite
            // already starts with the account name.
            let path = reqwest::Url::parse(&url)
                .map(|u| u.path().to_string())
                .unwrap_or(blob);
            let auth = azure_shared_key(
                &self.account,
                &self.secret,
                &path,
                body.len(),
                content_type,
                &[
                    ("x-ms-blob-type", "BlockBlob"),
                    ("x-ms-date", &date),
                    ("x-ms-version", AZURE_VERSION),
                ],
            )?;
            req = req.header("Authorization", auth);
        }
        let res = req
            .body(body)
            .send()
            .await
            .map_err(|e| CmsError::Storage(format!("Azure: {e}")))?;
        if res.status().is_success() {
            return Ok(());
        }
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        let code = text
            .split("<Code>")
            .nth(1)
            .and_then(|s| s.split("</Code>").next())
            .unwrap_or_default();
        Err(CmsError::Storage(format!(
            "Azure answered {status} {code} for `{key}`"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn azure_signature_is_stable() {
        // Known-answer vector computed with the documented algorithm.
        let auth = azure_shared_key(
            "myaccount",
            "a2V5", // "key"
            "/c/a%20b.json",
            2,
            "application/json",
            &[
                ("x-ms-version", "2021-08-06"),
                ("x-ms-date", "Mon, 01 Jan 2024 00:00:00 GMT"),
                ("x-ms-blob-type", "BlockBlob"),
            ],
        )
        .unwrap();
        let to_sign = "PUT\n\n\n2\n\napplication/json\n\n\n\n\n\n\nx-ms-blob-type:BlockBlob\nx-ms-date:Mon, 01 Jan 2024 00:00:00 GMT\nx-ms-version:2021-08-06\n/myaccount/c/a%20b.json";
        let mut mac = <Hmac<Sha256>>::new_from_slice(b"key").unwrap();
        mac.update(to_sign.as_bytes());
        assert_eq!(
            auth,
            format!(
                "SharedKey myaccount:{}",
                STANDARD.encode(mac.finalize().into_bytes())
            )
        );
        assert!(azure_shared_key("a", "!!", "/", 0, "", &[]).is_err());
        assert_eq!(
            encode_path("fr.lproj/a b+c.strings"),
            "fr.lproj/a%20b%2Bc.strings"
        );
    }

    #[test]
    fn prefixes() {
        assert_eq!(check_prefix("/i18n/v1/").unwrap(), "i18n/v1");
        assert!(check_prefix("a/../b").is_err());
        assert!(check_prefix("a b").is_err());
        assert_eq!(check_prefix("").unwrap(), "");
    }
}
