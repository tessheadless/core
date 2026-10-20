// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tess_core::model::{format_ts, now};
use tess_core::secrets::SecretBox;
use tess_core::{Actor, Cms, CmsError, HookRegistry, Result, Role, Storage, SystemEvent};

use crate::formats::{self, ExportFile, ExportOptions, Format};
use crate::{
    Language, Translation, TranslationKey, TranslationState, check_key_name, check_namespace,
    key_id, normalize_tag,
};

/// Pseudo content type limiting Contributors and Reviewers: users whose scope
/// is not empty need `i18n` in it to work on translations.
pub const SCOPE: &str = "i18n";

pub const I18N_EVENTS: &[&str] = &[
    "i18n.language.create",
    "i18n.language.update",
    "i18n.language.delete",
    "i18n.key.create",
    "i18n.key.update",
    "i18n.key.delete",
    "i18n.translation.update",
    "i18n.import",
    "i18n.publish",
];

const LANGUAGES_KEY: &str = "i18n/languages";
const SETTINGS_KEY: &str = "i18n/settings";
pub(crate) const KEYS_PREFIX: &str = "i18n/keys/";
const CACHE_TTL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct I18nConfig {
    /// Publish auto-publish targets within the request (AWS Lambda) instead
    /// of in the background after `debounce`.
    pub inline: bool,
    pub debounce: Duration,
}

impl Default for I18nConfig {
    fn default() -> Self {
        I18nConfig {
            inline: false,
            debounce: Duration::from_secs(3),
        }
    }
}

/// Instance-wide options (settings key `i18n/settings`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct I18nSettings {
    /// Serve `/i18n/{format}/{file}` without credentials.
    pub public: bool,
    /// Translation states served there.
    pub served_states: Vec<TranslationState>,
    /// Fill missing translations with the base language there.
    pub fallback_to_base: bool,
}

impl Default for I18nSettings {
    fn default() -> Self {
        I18nSettings {
            public: true,
            served_states: TranslationState::ALL.to_vec(),
            fallback_to_base: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct KeyFilter {
    /// Case-insensitive match on the name, description or any text.
    pub search: Option<String>,
    pub namespace: Option<String>,
    pub tag: Option<String>,
    /// With `state`: the language the state applies to (default: every
    /// language but the base one).
    pub language: Option<String>,
    /// `UNTRANSLATED`, `TRANSLATED` or `REVIEWED`.
    pub state: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct KeyInput {
    pub name: String,
    pub namespace: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub plural: bool,
    /// Initial texts `(language, text)`, saved as TRANSLATED.
    pub translations: Vec<(String, String)>,
}

/// `None` keeps the current value.
#[derive(Debug, Clone, Default)]
pub struct KeyUpdate {
    pub name: Option<String>,
    pub namespace: Option<String>,
    /// `Some(None)` clears it.
    pub description: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub plural: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportMode {
    /// Existing translations are left unchanged.
    Keep,
    /// Existing translations are replaced (REVIEWED ones too).
    Override,
}

#[derive(Debug, Clone)]
pub struct ImportInput {
    pub format: Format,
    pub content: String,
    /// Language of single-language files, unless the file declares it.
    pub language: Option<String>,
    pub namespace: String,
    pub mode: ImportMode,
    /// State given to imported texts; REVIEWED needs the review permission.
    pub state: TranslationState,
    /// Tags added to the keys the import creates.
    pub tags: Vec<String>,
    /// Add languages found in the file (Admin only); otherwise unknown
    /// languages are an error.
    pub create_languages: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub keys_created: u32,
    pub keys_updated: u32,
    pub translations_created: u32,
    pub translations_updated: u32,
    pub translations_skipped: u32,
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageStats {
    pub language: String,
    pub total: u32,
    pub translated: u32,
    pub reviewed: u32,
}

pub struct I18nService {
    pub(crate) storage: Arc<dyn Storage>,
    pub(crate) secrets: SecretBox,
    pub(crate) config: I18nConfig,
    hooks: OnceLock<HookRegistry>,
    cache: RwLock<Option<(Instant, Arc<Vec<TranslationKey>>)>>,
    pub(crate) auto: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Serializes read-modify-write cycles on this instance.
    pub(crate) write_lock: tokio::sync::Mutex<()>,
}

fn forbidden(msg: &str) -> CmsError {
    CmsError::Forbidden(msg.to_string())
}

pub(crate) fn can_edit(a: &Actor) -> bool {
    matches!(a.role, Role::Owner | Role::Admin | Role::Contributor) && a.in_scope(SCOPE)
}

pub(crate) fn can_review(a: &Actor) -> bool {
    matches!(a.role, Role::Owner | Role::Admin | Role::Reviewer) && a.in_scope(SCOPE)
}

fn require_edit(a: &Actor) -> Result<()> {
    if can_edit(a) {
        Ok(())
    } else {
        Err(forbidden("you cannot edit translations"))
    }
}

fn clean_tags(tags: Vec<String>) -> Result<Vec<String>> {
    let set: BTreeSet<String> = tags
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    if set.iter().any(|t| t.chars().count() > 50) {
        return Err(CmsError::validation("tags have at most 50 characters"));
    }
    Ok(set.into_iter().collect())
}

fn clean_description(d: Option<String>) -> Result<Option<String>> {
    let d = d.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
    if d.as_ref().is_some_and(|d| d.chars().count() > 2000) {
        return Err(CmsError::validation(
            "descriptions have at most 2000 characters",
        ));
    }
    Ok(d)
}

fn check_text(text: &str) -> Result<()> {
    if text.len() > 100_000 {
        return Err(CmsError::validation(
            "translations have at most 100 000 bytes",
        ));
    }
    Ok(())
}

impl I18nService {
    pub fn new(storage: Arc<dyn Storage>, secrets: SecretBox, config: I18nConfig) -> Self {
        I18nService {
            storage,
            secrets,
            config,
            hooks: OnceLock::new(),
            cache: RwLock::new(None),
            auto: Mutex::new(None),
            write_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// Connects the service to the CMS hooks, so translation changes reach
    /// webhooks like content changes do.
    pub fn install(&self, cms: &Cms) {
        let _ = self.hooks.set(cms.hooks().clone());
    }

    pub(crate) async fn emit(&self, event: &str, data: Value, actor: &Actor) {
        if let Some(hooks) = self.hooks.get() {
            hooks.notify(&SystemEvent::new(event, data, actor)).await;
        }
    }

    // ---- settings ------------------------------------------------------

    pub async fn settings(&self) -> Result<I18nSettings> {
        match self.storage.get_setting(SETTINGS_KEY).await? {
            Some(v) => serde_json::from_value(v).map_err(CmsError::storage),
            None => Ok(I18nSettings::default()),
        }
    }

    pub async fn update_settings(
        &self,
        settings: I18nSettings,
        actor: &Actor,
    ) -> Result<I18nSettings> {
        actor.require_admin()?;
        let v = serde_json::to_value(&settings).map_err(CmsError::storage)?;
        self.storage.put_setting(SETTINGS_KEY, &v).await?;
        Ok(settings)
    }

    // ---- languages -----------------------------------------------------

    pub async fn languages(&self) -> Result<Vec<Language>> {
        match self.storage.get_setting(LANGUAGES_KEY).await? {
            Some(v) => serde_json::from_value(v).map_err(CmsError::storage),
            None => Ok(vec![]),
        }
    }

    async fn save_languages(&self, mut langs: Vec<Language>) -> Result<()> {
        if !langs.is_empty() && !langs.iter().any(|l| l.base) {
            langs[0].base = true;
        }
        langs.sort_by(|a, b| b.base.cmp(&a.base).then(a.tag.cmp(&b.tag)));
        let v = serde_json::to_value(&langs).map_err(CmsError::storage)?;
        self.storage.put_setting(LANGUAGES_KEY, &v).await
    }

    pub async fn add_language(
        self: &Arc<Self>,
        tag: &str,
        name: &str,
        base: bool,
        actor: &Actor,
    ) -> Result<Language> {
        actor.require_admin()?;
        let _guard = self.write_lock.lock().await;
        let tag = normalize_tag(tag)?;
        let mut langs = self.languages().await?;
        if langs.iter().any(|l| l.tag == tag) {
            return Err(CmsError::Conflict(format!(
                "language `{tag}` already exists"
            )));
        }
        if base {
            langs.iter_mut().for_each(|l| l.base = false);
        }
        let name = if name.trim().is_empty() {
            tag.clone()
        } else {
            name.trim().to_string()
        };
        let lang = Language {
            tag,
            name,
            base: base || langs.is_empty(),
        };
        langs.push(lang.clone());
        self.save_languages(langs).await?;
        self.emit("i18n.language.create", json!(lang), actor).await;
        Ok(lang)
    }

    pub async fn update_language(
        self: &Arc<Self>,
        tag: &str,
        name: Option<String>,
        base: Option<bool>,
        actor: &Actor,
    ) -> Result<Language> {
        actor.require_admin()?;
        let _guard = self.write_lock.lock().await;
        let tag = normalize_tag(tag)?;
        let mut langs = self.languages().await?;
        if !langs.iter().any(|l| l.tag == tag) {
            return Err(CmsError::NotFound(format!("language `{tag}`")));
        }
        if base == Some(true) {
            langs.iter_mut().for_each(|l| l.base = l.tag == tag);
        } else if base == Some(false) && langs.iter().any(|l| l.tag == tag && l.base) {
            return Err(CmsError::validation(
                "make another language the base language instead",
            ));
        }
        let lang = langs.iter_mut().find(|l| l.tag == tag).expect("checked");
        if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
            lang.name = n.trim().to_string();
        }
        let lang = lang.clone();
        self.save_languages(langs).await?;
        self.emit("i18n.language.update", json!(lang), actor).await;
        Ok(lang)
    }

    /// Removes a language and all of its translations.
    pub async fn remove_language(self: &Arc<Self>, tag: &str, actor: &Actor) -> Result<()> {
        actor.require_admin()?;
        let _guard = self.write_lock.lock().await;
        let tag = normalize_tag(tag)?;
        let mut langs = self.languages().await?;
        let Some(pos) = langs.iter().position(|l| l.tag == tag) else {
            return Err(CmsError::NotFound(format!("language `{tag}`")));
        };
        if langs[pos].base && langs.len() > 1 {
            return Err(CmsError::validation(
                "make another language the base language before removing this one",
            ));
        }
        langs.remove(pos);
        for mut key in self.load_keys().await? {
            if key.translations.remove(&tag).is_some() {
                self.save_key(&key).await?;
            }
        }
        self.save_languages(langs).await?;
        self.invalidate();
        self.emit("i18n.language.delete", json!({ "tag": tag }), actor)
            .await;
        self.after_change().await;
        Ok(())
    }

    async fn require_language(&self, tag: &str) -> Result<String> {
        let tag = normalize_tag(tag)?;
        if self.languages().await?.iter().any(|l| l.tag == tag) {
            Ok(tag)
        } else {
            Err(CmsError::validation(format!(
                "language `{tag}` is not configured"
            )))
        }
    }

    // ---- keys ----------------------------------------------------------

    fn invalidate(&self) {
        *self.cache.write().expect("cache lock") = None;
    }

    async fn load_keys(&self) -> Result<Vec<TranslationKey>> {
        let mut keys: Vec<TranslationKey> = self
            .storage
            .list_settings(KEYS_PREFIX)
            .await?
            .into_iter()
            .map(|(_, v)| serde_json::from_value(v).map_err(CmsError::storage))
            .collect::<Result<_>>()?;
        keys.sort_by(|a, b| a.namespace.cmp(&b.namespace).then(a.name.cmp(&b.name)));
        Ok(keys)
    }

    /// Every key, sorted by namespace then name (cached a few seconds).
    pub async fn all_keys(&self) -> Result<Arc<Vec<TranslationKey>>> {
        if let Some((at, keys)) = self.cache.read().expect("cache lock").as_ref()
            && at.elapsed() < CACHE_TTL
        {
            return Ok(keys.clone());
        }
        let keys = Arc::new(self.load_keys().await?);
        *self.cache.write().expect("cache lock") = Some((Instant::now(), keys.clone()));
        Ok(keys)
    }

    async fn save_key(&self, key: &TranslationKey) -> Result<()> {
        let v = serde_json::to_value(key).map_err(CmsError::storage)?;
        self.storage
            .put_setting(&format!("{KEYS_PREFIX}{}", key.id()), &v)
            .await
    }

    async fn stored_key(&self, id: &str) -> Result<TranslationKey> {
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(CmsError::NotFound(format!("translation key `{id}`")));
        }
        let v = self
            .storage
            .get_setting(&format!("{KEYS_PREFIX}{id}"))
            .await?
            .ok_or_else(|| CmsError::NotFound(format!("translation key `{id}`")))?;
        serde_json::from_value(v).map_err(CmsError::storage)
    }

    pub async fn key(&self, id: &str) -> Result<TranslationKey> {
        self.stored_key(id).await
    }

    pub async fn namespaces(&self) -> Result<Vec<String>> {
        let mut all: Vec<String> = self
            .all_keys()
            .await?
            .iter()
            .map(|k| k.namespace.clone())
            .collect();
        all.dedup();
        Ok(all)
    }

    pub async fn tags(&self) -> Result<Vec<String>> {
        let set: BTreeSet<String> = self
            .all_keys()
            .await?
            .iter()
            .flat_map(|k| k.tags.iter().cloned())
            .collect();
        Ok(set.into_iter().collect())
    }

    /// Filtered, paginated keys and the total number of matches.
    pub async fn keys(
        &self,
        filter: &KeyFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<TranslationKey>, usize)> {
        let all = self.all_keys().await?;
        let base = self
            .languages()
            .await?
            .into_iter()
            .find(|l| l.base)
            .map(|l| l.tag);
        let search = filter
            .search
            .as_ref()
            .map(|s| s.to_lowercase())
            .filter(|s| !s.is_empty());
        let lang = filter.language.as_deref().map(normalize_tag).transpose()?;
        let langs = self.languages().await?;
        let matches_state = |k: &TranslationKey, wanted: &str| {
            let check = |l: &str| match (k.translations.get(l), wanted) {
                (None, "UNTRANSLATED") => true,
                (Some(t), s) => t.state.as_str() == s,
                _ => false,
            };
            match &lang {
                Some(l) => check(l),
                None => langs
                    .iter()
                    .filter(|l| Some(&l.tag) != base.as_ref())
                    .any(|l| check(&l.tag)),
            }
        };
        let hits: Vec<&TranslationKey> = all
            .iter()
            .filter(|k| {
                filter
                    .namespace
                    .as_ref()
                    .is_none_or(|ns| &k.namespace == ns)
            })
            .filter(|k| filter.tag.as_ref().is_none_or(|t| k.tags.contains(t)))
            .filter(|k| filter.state.as_deref().is_none_or(|s| matches_state(k, s)))
            .filter(|k| {
                search.as_ref().is_none_or(|s| {
                    k.name.to_lowercase().contains(s)
                        || k.description
                            .as_ref()
                            .is_some_and(|d| d.to_lowercase().contains(s))
                        || k.translations
                            .values()
                            .any(|t| t.text.to_lowercase().contains(s))
                })
            })
            .collect();
        let total = hits.len();
        Ok((
            hits.into_iter().skip(offset).take(limit).cloned().collect(),
            total,
        ))
    }

    pub async fn stats(&self) -> Result<Vec<LanguageStats>> {
        let keys = self.all_keys().await?;
        Ok(self
            .languages()
            .await?
            .into_iter()
            .map(|l| {
                let states: Vec<_> = keys
                    .iter()
                    .filter_map(|k| k.translations.get(&l.tag))
                    .map(|t| t.state)
                    .collect();
                LanguageStats {
                    total: keys.len() as u32,
                    translated: states.len() as u32,
                    reviewed: states
                        .iter()
                        .filter(|s| **s == TranslationState::Reviewed)
                        .count() as u32,
                    language: l.tag,
                }
            })
            .collect())
    }

    pub async fn create_key(
        self: &Arc<Self>,
        input: KeyInput,
        actor: &Actor,
    ) -> Result<TranslationKey> {
        require_edit(actor)?;
        let _guard = self.write_lock.lock().await;
        let key = self.new_key(input, actor).await?;
        self.save_key(&key).await?;
        self.invalidate();
        self.emit("i18n.key.create", key_json(&key), actor).await;
        self.after_change().await;
        Ok(key)
    }

    async fn new_key(&self, input: KeyInput, actor: &Actor) -> Result<TranslationKey> {
        check_key_name(&input.name)?;
        check_namespace(&input.namespace)?;
        if self
            .storage
            .get_setting(&format!(
                "{KEYS_PREFIX}{}",
                key_id(&input.namespace, &input.name)
            ))
            .await?
            .is_some()
        {
            return Err(CmsError::Conflict(format!(
                "key `{}` already exists",
                input.name
            )));
        }
        let ts = format_ts(now());
        let mut translations = BTreeMap::new();
        for (lang, text) in input.translations {
            let lang = self.require_language(&lang).await?;
            check_text(&text)?;
            if !text.is_empty() {
                let t = Translation {
                    text,
                    state: TranslationState::Translated,
                    updated_at: ts.clone(),
                    updated_by: actor.name.clone(),
                };
                translations.insert(lang, t);
            }
        }
        Ok(TranslationKey {
            name: input.name,
            namespace: input.namespace,
            description: clean_description(input.description)?,
            tags: clean_tags(input.tags)?,
            plural: input.plural,
            translations,
            created_at: ts.clone(),
            updated_at: ts,
        })
    }

    /// Renaming or moving a key changes its id.
    pub async fn update_key(
        self: &Arc<Self>,
        id: &str,
        update: KeyUpdate,
        actor: &Actor,
    ) -> Result<TranslationKey> {
        require_edit(actor)?;
        let _guard = self.write_lock.lock().await;
        let mut key = self.stored_key(id).await?;
        if let Some(name) = update.name {
            check_key_name(&name)?;
            key.name = name;
        }
        if let Some(ns) = update.namespace {
            check_namespace(&ns)?;
            key.namespace = ns;
        }
        if let Some(d) = update.description {
            key.description = clean_description(d)?;
        }
        if let Some(tags) = update.tags {
            key.tags = clean_tags(tags)?;
        }
        if let Some(p) = update.plural {
            key.plural = p;
        }
        key.updated_at = format_ts(now());
        let new_id = key.id();
        if new_id != id {
            if self
                .storage
                .get_setting(&format!("{KEYS_PREFIX}{new_id}"))
                .await?
                .is_some()
            {
                return Err(CmsError::Conflict(format!(
                    "key `{}` already exists",
                    key.name
                )));
            }
            self.save_key(&key).await?;
            self.storage
                .delete_setting(&format!("{KEYS_PREFIX}{id}"))
                .await?;
        } else {
            self.save_key(&key).await?;
        }
        self.invalidate();
        let mut data = key_json(&key);
        data["previousId"] = json!(id);
        self.emit("i18n.key.update", data, actor).await;
        self.after_change().await;
        Ok(key)
    }

    /// Moves keys to `namespace` (each gets a new id). All or nothing: fails
    /// when a key of the same name already exists there, or when two moved
    /// keys share a name. Returns how many keys moved (those already in
    /// `namespace` are left unchanged).
    pub async fn move_keys(
        self: &Arc<Self>,
        ids: &[String],
        namespace: &str,
        actor: &Actor,
    ) -> Result<u32> {
        require_edit(actor)?;
        check_namespace(namespace)?;
        let _guard = self.write_lock.lock().await;
        let mut moves = Vec::new();
        let mut names = BTreeSet::new();
        let mut conflicts = Vec::new();
        for id in ids.iter().collect::<BTreeSet<_>>() {
            let key = self.stored_key(id).await?;
            if key.namespace == namespace {
                continue;
            }
            let new_id = key_id(namespace, &key.name);
            if !names.insert(key.name.clone())
                || self
                    .storage
                    .get_setting(&format!("{KEYS_PREFIX}{new_id}"))
                    .await?
                    .is_some()
            {
                conflicts.push(key.name.clone());
            }
            moves.push((id.clone(), key));
        }
        if !conflicts.is_empty() {
            return Err(CmsError::Conflict(format!(
                "namespace `{namespace}` already has key(s) named {}",
                conflicts
                    .iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        let ts = format_ts(now());
        for (id, key) in &mut moves {
            key.namespace = namespace.to_string();
            key.updated_at = ts.clone();
            self.save_key(key).await?;
            self.storage
                .delete_setting(&format!("{KEYS_PREFIX}{id}"))
                .await?;
        }
        self.invalidate();
        for (id, key) in &moves {
            let mut data = key_json(key);
            data["previousId"] = json!(id);
            self.emit("i18n.key.update", data, actor).await;
        }
        if !moves.is_empty() {
            self.after_change().await;
        }
        Ok(moves.len() as u32)
    }

    pub async fn delete_keys(self: &Arc<Self>, ids: &[String], actor: &Actor) -> Result<u32> {
        require_edit(actor)?;
        let _guard = self.write_lock.lock().await;
        let mut deleted = 0;
        for id in ids {
            let Ok(key) = self.stored_key(id).await else {
                continue;
            };
            if self
                .storage
                .delete_setting(&format!("{KEYS_PREFIX}{id}"))
                .await?
            {
                deleted += 1;
                self.emit(
                    "i18n.key.delete",
                    json!({ "id": id, "name": key.name, "namespace": key.namespace }),
                    actor,
                )
                .await;
            }
        }
        self.invalidate();
        if deleted > 0 {
            self.after_change().await;
        }
        Ok(deleted)
    }

    /// Sets (or with `""` removes) a text and/or changes its review state.
    pub async fn set_translation(
        self: &Arc<Self>,
        id: &str,
        language: &str,
        text: Option<String>,
        state: Option<TranslationState>,
        actor: &Actor,
    ) -> Result<TranslationKey> {
        let lang = self.require_language(language).await?;
        let _guard = self.write_lock.lock().await;
        let mut key = self.stored_key(id).await?;
        let ts = format_ts(now());
        let reviewer = can_review(actor);
        match text {
            Some(text) if text.is_empty() => {
                require_edit(actor)?;
                key.translations.remove(&lang);
            }
            Some(text) => {
                require_edit(actor)?;
                check_text(&text)?;
                if state == Some(TranslationState::Reviewed) && !reviewer {
                    return Err(forbidden("you cannot mark translations as reviewed"));
                }
                let previous = key.translations.get(&lang);
                let unchanged = previous.is_some_and(|p| p.text == text);
                let new_state = match (state, unchanged) {
                    (Some(s), _) => s,
                    (None, true) => previous
                        .map(|p| p.state)
                        .unwrap_or(TranslationState::Translated),
                    // Editing a reviewed text sends it back to review.
                    (None, false) => TranslationState::Translated,
                };
                key.translations.insert(
                    lang.clone(),
                    Translation {
                        text,
                        state: new_state,
                        updated_at: ts.clone(),
                        updated_by: actor.name.clone(),
                    },
                );
            }
            None => {
                let Some(state) = state else { return Ok(key) };
                if !reviewer {
                    return Err(forbidden("you cannot change review states"));
                }
                let t = key.translations.get_mut(&lang).ok_or_else(|| {
                    CmsError::validation(format!("`{}` has no {lang} translation yet", key.name))
                })?;
                t.state = state;
                t.updated_at = ts.clone();
                t.updated_by = actor.name.clone();
            }
        }
        key.updated_at = ts;
        self.save_key(&key).await?;
        self.invalidate();
        let t = key.translations.get(&lang);
        let data = json!({
            "key": key_json(&key),
            "language": lang,
            "text": t.map(|t| &t.text),
            "state": t.map(|t| t.state.as_str()),
        });
        self.emit("i18n.translation.update", data, actor).await;
        self.after_change().await;
        Ok(key)
    }

    // ---- import / export -------------------------------------------------

    pub async fn import(
        self: &Arc<Self>,
        input: ImportInput,
        actor: &Actor,
    ) -> Result<ImportReport> {
        require_edit(actor)?;
        if input.state == TranslationState::Reviewed && !can_review(actor) {
            return Err(forbidden("you cannot import reviewed translations"));
        }
        if !input.format.importable() {
            return Err(CmsError::validation(format!(
                "{} files cannot be imported",
                input.format.as_str()
            )));
        }
        check_namespace(&input.namespace)?;
        let parsed = formats::parse(input.format, &input.content, input.language.as_deref())?;
        let _guard = self.write_lock.lock().await;

        let found: BTreeSet<String> = parsed
            .iter()
            .flat_map(|k| k.translations.iter().map(|(l, _)| l.clone()))
            .collect();
        let mut langs = self.languages().await?;
        let unknown: Vec<String> = found
            .iter()
            .filter(|l| !langs.iter().any(|x| &x.tag == *l))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            if !(input.create_languages && actor.role.is_admin()) {
                return Err(CmsError::validation(format!(
                    "unknown languages: {} (add them first, or let an Admin import with “create languages”)",
                    unknown.join(", ")
                )));
            }
            for tag in &unknown {
                let base = langs.is_empty();
                langs.push(Language {
                    tag: tag.clone(),
                    name: tag.clone(),
                    base,
                });
            }
            self.save_languages(langs).await?;
        }

        let tags = clean_tags(input.tags)?;
        let ts = format_ts(now());
        let mut report = ImportReport {
            languages: found.into_iter().collect(),
            ..Default::default()
        };
        for item in parsed {
            if check_key_name(&item.name).is_err() {
                report.translations_skipped += item.translations.len() as u32;
                continue;
            }
            let id = key_id(&input.namespace, &item.name);
            let (mut key, created) = match self.stored_key(&id).await {
                Ok(k) => (k, false),
                Err(CmsError::NotFound(_)) => (
                    TranslationKey {
                        name: item.name.clone(),
                        namespace: input.namespace.clone(),
                        description: None,
                        tags: tags.clone(),
                        plural: false,
                        translations: BTreeMap::new(),
                        created_at: ts.clone(),
                        updated_at: ts.clone(),
                    },
                    true,
                ),
                Err(e) => return Err(e),
            };
            let mut changed = created;
            if let Some(d) = clean_description(item.description)?
                && (key.description.is_none() || input.mode == ImportMode::Override)
                && key.description.as_ref() != Some(&d)
            {
                key.description = Some(d);
                changed = true;
                if !created {
                    report.keys_updated += 1;
                }
            }
            if item.plural && !key.plural {
                key.plural = true;
                changed = true;
            }
            for (lang, text) in item.translations {
                if text.is_empty() || check_text(&text).is_err() {
                    report.translations_skipped += 1;
                    continue;
                }
                match key.translations.get(&lang) {
                    Some(t) if t.text == text && t.state >= input.state => {
                        report.translations_skipped += 1
                    }
                    Some(_) if input.mode == ImportMode::Keep => report.translations_skipped += 1,
                    existing => {
                        if existing.is_some() {
                            report.translations_updated += 1;
                        } else {
                            report.translations_created += 1;
                        }
                        let t = Translation {
                            text,
                            state: input.state,
                            updated_at: ts.clone(),
                            updated_by: actor.name.clone(),
                        };
                        key.translations.insert(lang, t);
                        changed = true;
                    }
                }
            }
            if changed {
                key.updated_at = ts.clone();
                self.save_key(&key).await?;
            }
            if created {
                report.keys_created += 1;
            }
        }
        self.invalidate();
        self.emit(
            "i18n.import",
            json!({ "namespace": input.namespace, "format": input.format, "report": report }),
            actor,
        )
        .await;
        self.after_change().await;
        Ok(report)
    }

    pub async fn export(&self, opts: &ExportOptions) -> Result<Vec<ExportFile>> {
        let langs = self.languages().await?;
        let mut opts = opts.clone();
        opts.languages = opts
            .languages
            .iter()
            .map(|l| normalize_tag(l))
            .collect::<Result<_>>()?;
        formats::export(&self.all_keys().await?, &langs, &opts)
    }

    /// The file at `path` of the live export in `format` (`/i18n/{format}/{path}`).
    pub async fn serve(&self, format: Format, path: &str) -> Result<Option<ExportFile>> {
        let settings = self.settings().await?;
        let opts = ExportOptions {
            states: settings.served_states,
            fallback_to_base: settings.fallback_to_base,
            ..ExportOptions::new(format)
        };
        if self.languages().await?.is_empty() {
            return Ok(None);
        }
        Ok(self
            .export(&opts)
            .await?
            .into_iter()
            .find(|f| f.path == path))
    }
}

pub(crate) fn key_json(key: &TranslationKey) -> Value {
    let mut v = serde_json::to_value(key).unwrap_or_default();
    v["id"] = json!(key.id());
    v
}

#[cfg(test)]
mod tests {
    use tess_core::memory::MemoryStorage;

    use super::*;

    fn svc() -> Arc<I18nService> {
        Arc::new(I18nService::new(
            Arc::new(MemoryStorage::new()),
            SecretBox::new(&[3u8; 32]).unwrap(),
            I18nConfig::default(),
        ))
    }

    fn user(role: Role, scope: &[&str]) -> Actor {
        Actor {
            id: None,
            name: format!("{role:?}"),
            role,
            scope: scope.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn languages_keys_translations_and_review() {
        let s = svc();
        let admin = Actor::system();
        let contributor = user(Role::Contributor, &[]);
        let reviewer = user(Role::Reviewer, &["i18n"]);
        let outsider = user(Role::Contributor, &["Post"]);

        assert!(
            s.add_language("en", "English", false, &contributor)
                .await
                .is_err()
        );
        let en = s
            .add_language("en", "English", false, &admin)
            .await
            .unwrap();
        assert!(en.base, "the first language is the base one");
        s.add_language("fr_fr", "", false, &admin).await.unwrap();
        assert_eq!(s.languages().await.unwrap()[1].tag, "fr-FR");

        let input = KeyInput {
            name: "home.title".into(),
            namespace: "web".into(),
            translations: vec![("en".into(), "Home".into())],
            tags: vec!["ui".into(), "ui".into()],
            ..Default::default()
        };
        assert!(
            s.create_key(input.clone(), &outsider).await.is_err(),
            "scope without i18n"
        );
        let k = s.create_key(input.clone(), &contributor).await.unwrap();
        assert_eq!(k.tags, ["ui"]);
        assert!(matches!(
            s.create_key(input, &contributor).await,
            Err(CmsError::Conflict(_))
        ));

        let id = k.id();
        assert!(
            s.set_translation(&id, "de", Some("x".into()), None, &contributor)
                .await
                .is_err(),
            "unknown language"
        );
        s.set_translation(&id, "fr-FR", Some("Accueil".into()), None, &contributor)
            .await
            .unwrap();
        assert!(
            s.set_translation(
                &id,
                "fr-FR",
                None,
                Some(TranslationState::Reviewed),
                &contributor
            )
            .await
            .is_err(),
            "contributors do not review"
        );
        let k = s
            .set_translation(
                &id,
                "fr-FR",
                None,
                Some(TranslationState::Reviewed),
                &reviewer,
            )
            .await
            .unwrap();
        assert_eq!(k.translations["fr-FR"].state, TranslationState::Reviewed);
        let k = s
            .set_translation(&id, "fr-FR", Some("Accueil !".into()), None, &contributor)
            .await
            .unwrap();
        assert_eq!(
            k.translations["fr-FR"].state,
            TranslationState::Translated,
            "edits go back to review"
        );

        let (hits, total) = s
            .keys(
                &KeyFilter {
                    state: Some("UNTRANSLATED".into()),
                    ..Default::default()
                },
                0,
                10,
            )
            .await
            .unwrap();
        assert_eq!((hits.len(), total), (0, 0));
        let (_, total) = s
            .keys(
                &KeyFilter {
                    search: Some("accueil".into()),
                    ..Default::default()
                },
                0,
                10,
            )
            .await
            .unwrap();
        assert_eq!(total, 1);

        // Rename moves the key.
        let k = s
            .update_key(
                &id,
                KeyUpdate {
                    name: Some("home.heading".into()),
                    ..Default::default()
                },
                &contributor,
            )
            .await
            .unwrap();
        assert_ne!(k.id(), id);
        assert!(s.key(&id).await.is_err());
        let stats = s.stats().await.unwrap();
        assert_eq!(
            (
                stats[1].language.as_str(),
                stats[1].translated,
                stats[1].total
            ),
            ("fr-FR", 1, 1)
        );

        assert!(
            s.remove_language("en", &admin).await.is_err(),
            "base language"
        );
        s.remove_language("fr-FR", &admin).await.unwrap();
        assert!(
            !s.key(&k.id())
                .await
                .unwrap()
                .translations
                .contains_key("fr-FR")
        );
        assert_eq!(s.delete_keys(&[k.id()], &contributor).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn import_modes_languages_and_serving() {
        let s = svc();
        let admin = Actor::system();
        let contributor = user(Role::Contributor, &[]);
        s.add_language("en", "English", true, &admin).await.unwrap();
        let import = |content: &str, mode, create_languages, format| ImportInput {
            format,
            content: content.into(),
            language: Some("en".into()),
            namespace: String::new(),
            mode,
            state: TranslationState::Translated,
            tags: vec!["imported".into()],
            create_languages,
        };
        let r = s
            .import(
                import(
                    r#"{"a": "A", "b": {"c": "C"}}"#,
                    ImportMode::Keep,
                    false,
                    Format::Json,
                ),
                &contributor,
            )
            .await
            .unwrap();
        assert_eq!((r.keys_created, r.translations_created), (2, 2));
        let r = s
            .import(
                import(r#"{"a": "A2"}"#, ImportMode::Keep, false, Format::Json),
                &contributor,
            )
            .await
            .unwrap();
        assert_eq!((r.translations_skipped, r.translations_updated), (1, 0));
        let r = s
            .import(
                import(r#"{"a": "A2"}"#, ImportMode::Override, false, Format::Json),
                &contributor,
            )
            .await
            .unwrap();
        assert_eq!(r.translations_updated, 1);

        let csv = "key,en,de\na,A2,A-de\n";
        assert!(
            s.import(
                import(csv, ImportMode::Keep, true, Format::Csv),
                &contributor
            )
            .await
            .is_err(),
            "admin only"
        );
        let r = s
            .import(import(csv, ImportMode::Keep, true, Format::Csv), &admin)
            .await
            .unwrap();
        assert_eq!(r.translations_created, 1);
        assert_eq!(s.languages().await.unwrap().len(), 2);

        let file = s
            .serve(Format::JsonNested, "de.json")
            .await
            .unwrap()
            .unwrap();
        let v: Value = serde_json::from_str(&file.content).unwrap();
        assert_eq!(
            v,
            json!({"a": "A-de", "b": {"c": "C"}}),
            "fallback to the base language"
        );
        assert!(s.serve(Format::Json, "xx.json").await.unwrap().is_none());
        let (keys, _) = s
            .keys(
                &KeyFilter {
                    tag: Some("imported".into()),
                    ..Default::default()
                },
                0,
                10,
            )
            .await
            .unwrap();
        assert_eq!(keys.len(), 2);
    }

    #[tokio::test]
    async fn move_keys_to_another_namespace() {
        let s = svc();
        let admin = Actor::system();
        s.add_language("en", "English", false, &admin)
            .await
            .unwrap();
        let key = |name: &str, namespace: &str| KeyInput {
            name: name.into(),
            namespace: namespace.into(),
            description: None,
            tags: vec![],
            plural: false,
            translations: vec![("en".into(), format!("{namespace}:{name}"))],
        };
        let a = s.create_key(key("a", "web"), &admin).await.unwrap();
        let b = s.create_key(key("b", "web"), &admin).await.unwrap();
        let b_app = s.create_key(key("b", "app"), &admin).await.unwrap();
        let c = s.create_key(key("c", "app"), &admin).await.unwrap();

        assert!(s.move_keys(&[a.id()], "bad ns", &admin).await.is_err());
        assert!(
            s.move_keys(&[a.id()], "app", &user(Role::Reviewer, &["i18n"]))
                .await
                .is_err()
        );
        // `b` exists in both: nothing moves.
        let err = s
            .move_keys(&[a.id(), b.id()], "app", &admin)
            .await
            .unwrap_err();
        assert!(
            matches!(&err, CmsError::Conflict(m) if m.contains("`b`")),
            "{err}"
        );
        let err = s
            .move_keys(&[b.id(), b_app.id()], "mobile", &admin)
            .await
            .unwrap_err();
        assert!(
            matches!(err, CmsError::Conflict(_)),
            "two moved keys named `b`"
        );
        assert_eq!(s.namespaces().await.unwrap(), vec!["app", "web"]);

        // Keys already there are skipped; translations follow the key.
        assert_eq!(
            s.move_keys(&[a.id(), c.id()], "app", &admin).await.unwrap(),
            1
        );
        let moved = s.key(&key_id("app", "a")).await.unwrap();
        assert_eq!(moved.translations["en"].text, "web:a");
        assert!(s.key(&a.id()).await.is_err(), "old id is gone");
        assert_eq!(s.all_keys().await.unwrap().len(), 4);
    }
}
