// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Translation management, in the spirit of Tolgee.
//!
//! - **Languages** (one of them the *base* language translators start from).
//! - **Keys** (`home.title`), grouped in optional **namespaces** (`common`,
//!   `checkout`), with a description, tags and a plural flag. Texts use ICU
//!   MessageFormat (`{count, plural, one {# item} other {# items}}`).
//! - One **translation** per key and language, with a review **state**
//!   (`TRANSLATED`, then `REVIEWED`); a missing translation is untranslated.
//! - **Import / export** in many formats ([`Format`]), live **serving** under
//!   `/i18n/{format}/{file}`, and **delivery targets** that publish the
//!   exported files to S3-compatible buckets (Garage, AWS S3, Cloudflare R2,
//!   MinIO) or Azure Blob Storage, on demand or after every change.
//!
//! Everything lives in the settings store of the [`tess_core::Storage`]
//! backend (`i18n/...`), so every database works.

mod delivery;
mod formats;
mod icu;
mod service;

pub use delivery::{
    AzureAuth, DeliveryInput, DeliveryTarget, DeliveryUpdate, PublishReport, StorageKind,
};
pub use formats::{ExportFile, ExportOptions, Format, ImportedKey, export, parse, zip_files};
pub use service::{
    I18N_EVENTS, I18nConfig, I18nService, I18nSettings, ImportInput, ImportMode, ImportReport,
    KeyFilter, KeyInput, KeyUpdate, LanguageStats, SCOPE,
};

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tess_core::{CmsError, Result};

static LANG_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})*$").unwrap());
static NAMESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_-]{0,64}$").unwrap());

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    /// BCP 47 tag, e.g. `en`, `fr-CA`, `zh-Hant`.
    pub tag: String,
    pub name: String,
    /// The source language translators work from (exactly one).
    pub base: bool,
}

/// Normalizes `en_us` / `EN-us` to `en-US`.
pub fn normalize_tag(tag: &str) -> Result<String> {
    let tag = tag.trim().replace('_', "-");
    if !LANG_TAG.is_match(&tag) {
        return Err(CmsError::validation(format!(
            "`{tag}` is not a valid language tag (e.g. en, fr-CA)"
        )));
    }
    let mut parts = tag.split('-');
    let mut out = parts.next().unwrap_or_default().to_ascii_lowercase();
    for p in parts {
        out.push('-');
        out.push_str(&match p.len() {
            2 => p.to_ascii_uppercase(),
            4 => {
                let mut s = p.to_ascii_lowercase();
                s[..1].make_ascii_uppercase();
                s
            }
            _ => p.to_string(),
        });
    }
    Ok(out)
}

pub fn check_namespace(ns: &str) -> Result<()> {
    if NAMESPACE.is_match(ns) {
        Ok(())
    } else {
        Err(CmsError::validation(format!(
            "namespace `{ns}`: use at most 64 letters, digits, `_` or `-`"
        )))
    }
}

pub fn check_key_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.chars().count() > 255 || name.chars().any(char::is_control) {
        return Err(CmsError::validation(
            "key names must have 1 to 255 characters, without control characters",
        ));
    }
    if name != name.trim() {
        return Err(CmsError::validation(format!(
            "key `{name}` starts or ends with spaces"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TranslationState {
    Translated,
    Reviewed,
}

impl TranslationState {
    pub const ALL: [TranslationState; 2] =
        [TranslationState::Translated, TranslationState::Reviewed];

    pub fn as_str(self) -> &'static str {
        match self {
            TranslationState::Translated => "TRANSLATED",
            TranslationState::Reviewed => "REVIEWED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    pub text: String,
    pub state: TranslationState,
    pub updated_at: String,
    pub updated_by: String,
}

/// A translation key (settings key `i18n/keys/{id}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationKey {
    pub name: String,
    /// `""` is the default namespace.
    #[serde(default)]
    pub namespace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Texts are ICU plural messages; exported as plural forms where the
    /// format supports it (i18next, Android).
    #[serde(default)]
    pub plural: bool,
    /// By language tag.
    #[serde(default)]
    pub translations: BTreeMap<String, Translation>,
    pub created_at: String,
    pub updated_at: String,
}

impl TranslationKey {
    pub fn id(&self) -> String {
        key_id(&self.namespace, &self.name)
    }

    pub fn text(&self, lang: &str) -> Option<&str> {
        self.translations.get(lang).map(|t| t.text.as_str())
    }
}

/// Stable id of a key: derived from its namespace and name, so a key can
/// only exist once.
pub fn key_id(namespace: &str, name: &str) -> String {
    let digest = Sha256::new_with_prefix(namespace.as_bytes())
        .chain_update([0u8])
        .chain_update(name.as_bytes())
        .finalize();
    hex::encode(&digest[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_and_ids() {
        assert_eq!(normalize_tag("en").unwrap(), "en");
        assert_eq!(normalize_tag("fr_ca").unwrap(), "fr-CA");
        assert_eq!(normalize_tag("ZH-hant-tw").unwrap(), "zh-Hant-TW");
        assert!(normalize_tag("english!").is_err());
        assert!(normalize_tag("../x").is_err());
        assert_ne!(key_id("", "a.b"), key_id("a", "b"));
        assert_eq!(key_id("ns", "k").len(), 32);
        assert!(check_namespace("common").is_ok());
        assert!(check_namespace("a/b").is_err());
        assert!(check_key_name(" x").is_err());
    }
}
