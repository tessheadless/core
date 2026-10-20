// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! File formats for import and export.
//!
//! | format          | file layout (namespace `ns`, language `fr-CA`) | plurals |
//! |-----------------|------------------------------------------------|---------|
//! | `JSON`          | `ns/fr-CA.json`, flat `{"home.title": "…"}`    | ICU text |
//! | `JSON_NESTED`   | `ns/fr-CA.json`, `{"home": {"title": "…"}}`    | ICU text |
//! | `I18NEXT`       | `fr-CA/ns.json` (`translation.json` by default), nested, `{{var}}` | `key_one`, `key_other` |
//! | `ARB` (Flutter) | `ns/app_fr_CA.arb`                             | ICU text |
//! | `XLIFF` (1.2)   | `ns/fr-CA.xliff`, source = base language       | ICU text |
//! | `PO` (gettext)  | `ns/fr-CA.po`, `msgid` = key                   | ICU text |
//! | `PROPERTIES`    | `ns/messages_fr_CA.properties`                 | ICU text |
//! | `ANDROID_XML`   | `ns/values-fr-rCA/strings.xml`                 | `<plurals>` |
//! | `APPLE_STRINGS` | `fr-CA.lproj/ns.strings` (`Localizable.strings` by default) | ICU text |
//! | `YAML`          | `ns/fr-CA.yml`, nested under the language (Rails) | ICU text |
//! | `CSV`           | `ns.csv` (`translations.csv` by default), one column per language | ICU text |
//!
//! Every format can be imported back, except YAML (export only).

mod read;
mod write;

pub use read::{ImportedKey, parse};

use std::io::Write as _;

use serde::Serialize;
use tess_core::{CmsError, Result};

use crate::{Language, TranslationKey, TranslationState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Format {
    Json,
    JsonNested,
    I18next,
    Arb,
    Xliff,
    Po,
    Properties,
    AndroidXml,
    AppleStrings,
    Yaml,
    Csv,
}

impl Format {
    pub const ALL: [Format; 11] = [
        Format::Json,
        Format::JsonNested,
        Format::I18next,
        Format::Arb,
        Format::Xliff,
        Format::Po,
        Format::Properties,
        Format::AndroidXml,
        Format::AppleStrings,
        Format::Yaml,
        Format::Csv,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Format::Json => "JSON",
            Format::JsonNested => "JSON_NESTED",
            Format::I18next => "I18NEXT",
            Format::Arb => "ARB",
            Format::Xliff => "XLIFF",
            Format::Po => "PO",
            Format::Properties => "PROPERTIES",
            Format::AndroidXml => "ANDROID_XML",
            Format::AppleStrings => "APPLE_STRINGS",
            Format::Yaml => "YAML",
            Format::Csv => "CSV",
        }
    }

    pub fn parse(s: &str) -> Option<Format> {
        Format::ALL
            .into_iter()
            .find(|f| f.as_str().eq_ignore_ascii_case(s) || f.slug() == s)
    }

    /// Lowercase name used in URLs (`/i18n/json-nested/fr.json`).
    pub fn slug(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::JsonNested => "json-nested",
            Format::I18next => "i18next",
            Format::Arb => "arb",
            Format::Xliff => "xliff",
            Format::Po => "po",
            Format::Properties => "properties",
            Format::AndroidXml => "android",
            Format::AppleStrings => "apple",
            Format::Yaml => "yaml",
            Format::Csv => "csv",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Format::Json | Format::JsonNested | Format::I18next | Format::Arb => {
                "application/json; charset=utf-8"
            }
            Format::Xliff => "application/xliff+xml; charset=utf-8",
            Format::Po => "text/x-gettext-translation; charset=utf-8",
            Format::Properties => "text/x-java-properties; charset=utf-8",
            Format::AndroidXml => "application/xml; charset=utf-8",
            Format::AppleStrings => "text/plain; charset=utf-8",
            Format::Yaml => "application/yaml; charset=utf-8",
            Format::Csv => "text/csv; charset=utf-8",
        }
    }

    /// One file holds every language.
    pub fn multi_language(self) -> bool {
        matches!(self, Format::Csv)
    }

    pub fn importable(self) -> bool {
        !matches!(self, Format::Yaml)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFile {
    pub path: String,
    pub content_type: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub format: Format,
    /// Empty means every language.
    pub languages: Vec<String>,
    /// Empty means every namespace.
    pub namespaces: Vec<String>,
    /// Translations exported; others are treated as missing.
    pub states: Vec<TranslationState>,
    /// Fill missing translations with the base language text.
    pub fallback_to_base: bool,
}

impl ExportOptions {
    pub fn new(format: Format) -> Self {
        ExportOptions {
            format,
            languages: vec![],
            namespaces: vec![],
            states: TranslationState::ALL.to_vec(),
            fallback_to_base: false,
        }
    }
}

/// One exported message.
#[derive(Debug, Clone)]
pub(crate) struct Message<'a> {
    pub key: &'a TranslationKey,
    pub text: String,
}

/// `fr-CA` → `fr_CA`.
pub(crate) fn underscored(tag: &str) -> String {
    tag.replace('-', "_")
}

/// Android resource qualifier: `fr` → `fr`, `fr-CA` → `fr-rCA`, others BCP 47 (`b+zh+Hant`).
pub(crate) fn android_qualifier(tag: &str) -> String {
    let parts: Vec<&str> = tag.split('-').collect();
    match parts.as_slice() {
        [lang] => lang.to_string(),
        [lang, region] if region.len() == 2 => format!("{lang}-r{region}"),
        _ => format!("b+{}", parts.join("+")),
    }
}

fn path_for(format: Format, ns: &str, lang: &str) -> String {
    let dir = if ns.is_empty() {
        String::new()
    } else {
        format!("{ns}/")
    };
    match format {
        Format::Json | Format::JsonNested => format!("{dir}{lang}.json"),
        Format::I18next => format!(
            "{lang}/{}.json",
            if ns.is_empty() { "translation" } else { ns }
        ),
        Format::Arb => format!("{dir}app_{}.arb", underscored(lang)),
        Format::Xliff => format!("{dir}{lang}.xliff"),
        Format::Po => format!("{dir}{lang}.po"),
        Format::Properties => format!("{dir}messages_{}.properties", underscored(lang)),
        Format::AndroidXml => format!("{dir}values-{}/strings.xml", android_qualifier(lang)),
        Format::AppleStrings => format!(
            "{lang}.lproj/{}.strings",
            if ns.is_empty() { "Localizable" } else { ns }
        ),
        Format::Yaml => format!("{dir}{lang}.yml"),
        Format::Csv => format!("{}.csv", if ns.is_empty() { "translations" } else { ns }),
    }
}

fn text_for(
    key: &TranslationKey,
    lang: &str,
    base: Option<&str>,
    opts: &ExportOptions,
) -> Option<String> {
    let pick = |l: &str| {
        key.translations
            .get(l)
            .filter(|t| opts.states.contains(&t.state))
            .map(|t| t.text.clone())
    };
    pick(lang).or_else(|| {
        if opts.fallback_to_base {
            base.and_then(pick)
        } else {
            None
        }
    })
}

/// Exports `keys` (sorted by namespace then name).
pub fn export(
    keys: &[TranslationKey],
    languages: &[Language],
    opts: &ExportOptions,
) -> Result<Vec<ExportFile>> {
    let base = languages.iter().find(|l| l.base).map(|l| l.tag.as_str());
    let langs: Vec<&Language> = languages
        .iter()
        .filter(|l| opts.languages.is_empty() || opts.languages.iter().any(|t| t == &l.tag))
        .collect();
    if langs.is_empty() {
        return Err(CmsError::validation("no language to export"));
    }
    let mut namespaces: Vec<&str> = keys.iter().map(|k| k.namespace.as_str()).collect();
    namespaces.sort();
    namespaces.dedup();
    namespaces.retain(|ns| opts.namespaces.is_empty() || opts.namespaces.iter().any(|n| n == ns));
    if namespaces.is_empty() {
        namespaces.push(""); // still produce (empty) files
    }

    let mut files = vec![];
    for ns in namespaces {
        let ns_keys: Vec<&TranslationKey> = keys.iter().filter(|k| k.namespace == ns).collect();
        if opts.format.multi_language() {
            let content = write::csv(&ns_keys, &langs, |k, l| text_for(k, l, base, opts));
            files.push(ExportFile {
                path: path_for(opts.format, ns, ""),
                content_type: opts.format.content_type().into(),
                content,
            });
            continue;
        }
        for lang in &langs {
            let messages: Vec<Message> = ns_keys
                .iter()
                .filter_map(|k| {
                    Some(Message {
                        key: k,
                        text: text_for(k, &lang.tag, base, opts)?,
                    })
                })
                .collect();
            let content = match opts.format {
                Format::Json => write::json_flat(&messages),
                Format::JsonNested => write::json_nested(&messages, false),
                Format::I18next => write::json_nested(&messages, true),
                Format::Arb => write::arb(&messages, &lang.tag),
                Format::Xliff => {
                    let source = base.unwrap_or(&lang.tag);
                    write::xliff(&ns_keys, ns, source, &lang.tag, |k| {
                        (
                            k.text(source).map(String::from),
                            text_for(k, &lang.tag, base, opts),
                        )
                    })
                }
                Format::Po => write::po(&messages, &lang.tag),
                Format::Properties => write::properties(&messages),
                Format::AndroidXml => write::android(&messages),
                Format::AppleStrings => write::apple(&messages),
                Format::Yaml => write::yaml(&messages, &lang.tag),
                Format::Csv => unreachable!("multi-language"),
            };
            files.push(ExportFile {
                path: path_for(opts.format, ns, &lang.tag),
                content_type: opts.format.content_type().into(),
                content,
            });
        }
    }
    Ok(files)
}

/// Packs files into a zip archive.
pub fn zip_files(files: &[ExportFile]) -> Result<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for f in files {
        zip.start_file(f.path.as_str(), options)
            .map_err(CmsError::storage)?;
        zip.write_all(f.content.as_bytes())
            .map_err(CmsError::storage)?;
    }
    Ok(zip.finish().map_err(CmsError::storage)?.into_inner())
}

#[cfg(test)]
mod tests;
