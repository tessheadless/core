// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use tess_core::model::{format_ts, now};
use tess_core::{Actor, Cms, CmsError, HookRegistry, Result, Role, Storage, SystemEvent};
use uuid::Uuid;

use crate::{FormDef, FormField, HONEYPOT, Submission, check_definition, validate_submission};

const DEFS_PREFIX: &str = "forms/defs/";
const SUBMISSIONS_PREFIX: &str = "form-submissions/";

#[derive(Debug, Clone)]
pub struct FormsConfig {
    /// Submissions accepted per visitor and form, per minute.
    pub rate_per_minute: u32,
}

impl Default for FormsConfig {
    fn default() -> Self {
        FormsConfig {
            rate_per_minute: 10,
        }
    }
}

/// What Admins set on a form.
#[derive(Debug, Clone, Default)]
pub struct FormInput {
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub fields: Vec<FormField>,
    pub enabled: bool,
    pub success_message: Option<String>,
    pub redirect_url: Option<String>,
}

/// About a public submission.
#[derive(Debug, Clone, Default)]
pub struct SubmitMeta {
    /// Who submits (client IP), for the rate limit only: not stored.
    pub client: String,
    /// The page the form was posted from.
    pub source: Option<String>,
}

#[derive(Debug)]
pub enum SubmitError {
    /// No enabled form with this slug.
    NotFound,
    RateLimited,
    /// An error message per field.
    Invalid(BTreeMap<String, String>),
    Storage(CmsError),
}

/// Fixed one-minute windows per (client, form).
#[derive(Default)]
pub struct RateLimiter {
    windows: Mutex<HashMap<(String, Uuid), (Instant, u32)>>,
}

impl RateLimiter {
    fn allow(&self, client: &str, form: Uuid, limit: u32) -> bool {
        let mut windows = self.windows.lock().expect("rate limiter lock");
        let now = Instant::now();
        if windows.len() > 10_000 {
            windows.retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(60));
        }
        let (start, count) = windows
            .entry((client.to_string(), form))
            .or_insert((now, 0));
        if now.duration_since(*start) >= Duration::from_secs(60) {
            *start = now;
            *count = 0;
        }
        *count += 1;
        *count <= limit
    }
}

pub struct FormsService {
    storage: Arc<dyn Storage>,
    config: FormsConfig,
    limiter: RateLimiter,
    hooks: OnceLock<HookRegistry>,
}

fn require_admin(actor: &Actor) -> Result<()> {
    actor.require_admin()
}

fn clean(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Submissions are listed newest first: keys sort by reversed time.
fn submission_key(form: Uuid, submitted_ms: i64, id: Uuid) -> String {
    format!(
        "{SUBMISSIONS_PREFIX}{form}/{:020}-{id}",
        u64::MAX / 2 - submitted_ms.max(0) as u64
    )
}

fn form_json(f: &FormDef) -> Value {
    json!({ "id": f.id, "slug": f.slug, "title": f.title })
}

impl FormsService {
    pub fn new(storage: Arc<dyn Storage>, config: FormsConfig) -> Self {
        FormsService {
            storage,
            config,
            limiter: RateLimiter::default(),
            hooks: OnceLock::new(),
        }
    }

    /// Lets the service emit events (webhooks, n8n).
    pub fn install(&self, cms: &Cms) {
        let _ = self.hooks.set(cms.hooks().clone());
    }

    async fn emit(&self, event: &str, data: Value, actor: &Actor) {
        if let Some(hooks) = self.hooks.get() {
            hooks.notify(&SystemEvent::new(event, data, actor)).await;
        }
    }

    // ---- definitions ---------------------------------------------------

    pub async fn list(&self) -> Result<Vec<FormDef>> {
        let mut forms: Vec<FormDef> = self
            .storage
            .list_settings(DEFS_PREFIX)
            .await?
            .into_iter()
            .map(|(_, v)| serde_json::from_value(v).map_err(CmsError::storage))
            .collect::<Result<_>>()?;
        forms.sort_by_key(|f| f.title.to_lowercase());
        Ok(forms)
    }

    pub async fn get(&self, id: Uuid) -> Result<FormDef> {
        match self
            .storage
            .get_setting(&format!("{DEFS_PREFIX}{id}"))
            .await?
        {
            Some(v) => serde_json::from_value(v).map_err(CmsError::storage),
            None => Err(CmsError::NotFound(format!("form {id}"))),
        }
    }

    /// An enabled form, for visitors.
    pub async fn public(&self, slug: &str) -> Result<Option<FormDef>> {
        Ok(self
            .list()
            .await?
            .into_iter()
            .find(|f| f.slug == slug && f.enabled))
    }

    async fn check(&self, input: &FormInput, id: Option<Uuid>) -> Result<()> {
        check_definition(&input.slug, &input.title, &input.fields)?;
        if let Some(url) = &input.redirect_url
            && !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with('/'))
        {
            return Err(CmsError::validation(
                "the redirect URL must be an http(s) URL or a path",
            ));
        }
        if self
            .list()
            .await?
            .iter()
            .any(|f| f.slug == input.slug && Some(f.id) != id)
        {
            return Err(CmsError::Conflict(format!(
                "a form `{}` already exists",
                input.slug
            )));
        }
        Ok(())
    }

    async fn save(&self, form: &FormDef) -> Result<()> {
        let value = serde_json::to_value(form).map_err(CmsError::storage)?;
        self.storage
            .put_setting(&format!("{DEFS_PREFIX}{}", form.id), &value)
            .await
    }

    pub async fn create(&self, mut input: FormInput, actor: &Actor) -> Result<FormDef> {
        require_admin(actor)?;
        input.redirect_url = clean(input.redirect_url);
        self.check(&input, None).await?;
        let ts = format_ts(now());
        let form = FormDef {
            id: Uuid::new_v4(),
            slug: input.slug,
            title: input.title.trim().to_string(),
            description: clean(input.description),
            fields: input.fields,
            enabled: input.enabled,
            success_message: clean(input.success_message),
            redirect_url: input.redirect_url,
            created_at: ts.clone(),
            updated_at: ts,
            created_by: actor.name.clone(),
        };
        self.save(&form).await?;
        self.emit("form.create", form_json(&form), actor).await;
        Ok(form)
    }

    pub async fn update(&self, id: Uuid, mut input: FormInput, actor: &Actor) -> Result<FormDef> {
        require_admin(actor)?;
        let mut form = self.get(id).await?;
        input.redirect_url = clean(input.redirect_url);
        self.check(&input, Some(id)).await?;
        form.slug = input.slug;
        form.title = input.title.trim().to_string();
        form.description = clean(input.description);
        form.fields = input.fields;
        form.enabled = input.enabled;
        form.success_message = clean(input.success_message);
        form.redirect_url = input.redirect_url;
        form.updated_at = format_ts(now());
        self.save(&form).await?;
        self.emit("form.update", form_json(&form), actor).await;
        Ok(form)
    }

    /// Deletes a form and its submissions.
    pub async fn delete(&self, id: Uuid, actor: &Actor) -> Result<()> {
        require_admin(actor)?;
        let form = self.get(id).await?;
        for (key, _) in self
            .storage
            .list_settings(&format!("{SUBMISSIONS_PREFIX}{id}/"))
            .await?
        {
            self.storage.delete_setting(&key).await?;
        }
        self.storage
            .delete_setting(&format!("{DEFS_PREFIX}{id}"))
            .await?;
        self.emit("form.delete", form_json(&form), actor).await;
        Ok(())
    }

    // ---- submissions ---------------------------------------------------

    /// A visitor's submission: validated, stored, then `form.submitted` is
    /// emitted. A filled honeypot is accepted but dropped (`Ok(None)`), so
    /// bots don't learn they were caught.
    pub async fn submit(
        &self,
        slug: &str,
        raw: &Map<String, Value>,
        meta: SubmitMeta,
    ) -> std::result::Result<(FormDef, Option<Submission>), SubmitError> {
        let form = self
            .public(slug)
            .await
            .map_err(SubmitError::Storage)?
            .ok_or(SubmitError::NotFound)?;
        if !self
            .limiter
            .allow(&meta.client, form.id, self.config.rate_per_minute)
        {
            return Err(SubmitError::RateLimited);
        }
        if raw
            .get(HONEYPOT)
            .is_some_and(|v| !matches!(v, Value::Null) && v.as_str() != Some(""))
        {
            return Ok((form, None));
        }
        let data = validate_submission(&form.fields, raw).map_err(SubmitError::Invalid)?;
        let at = now();
        let submission = Submission {
            id: Uuid::new_v4(),
            form_id: form.id,
            data,
            submitted_at: format_ts(at),
            source: meta.source.filter(|s| s.len() <= 2_000),
        };
        let value = serde_json::to_value(&submission)
            .map_err(|e| SubmitError::Storage(CmsError::storage(e)))?;
        self.storage
            .put_setting(
                &submission_key(form.id, at.timestamp_millis(), submission.id),
                &value,
            )
            .await
            .map_err(SubmitError::Storage)?;
        let visitor = Actor {
            id: None,
            name: "visitor".into(),
            role: Role::Member,
            scope: vec![],
        };
        let data = json!({ "form": form_json(&form), "submission": submission });
        self.emit("form.submitted", data, &visitor).await;
        Ok((form, Some(submission)))
    }

    /// Newest first.
    pub async fn submissions(
        &self,
        form: Uuid,
        offset: usize,
        limit: usize,
        actor: &Actor,
    ) -> Result<(Vec<Submission>, usize)> {
        require_admin(actor)?;
        self.get(form).await?;
        let all = self
            .storage
            .list_settings(&format!("{SUBMISSIONS_PREFIX}{form}/"))
            .await?;
        let total = all.len();
        let items = all
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(_, v)| serde_json::from_value(v).map_err(CmsError::storage))
            .collect::<Result<_>>()?;
        Ok((items, total))
    }

    pub async fn submission_count(&self, form: Uuid) -> Result<usize> {
        Ok(self
            .storage
            .list_settings(&format!("{SUBMISSIONS_PREFIX}{form}/"))
            .await?
            .len())
    }

    /// Returns how many were deleted.
    pub async fn delete_submissions(&self, form: Uuid, ids: &[Uuid], actor: &Actor) -> Result<u32> {
        require_admin(actor)?;
        let mut deleted = 0;
        for (key, _) in self
            .storage
            .list_settings(&format!("{SUBMISSIONS_PREFIX}{form}/"))
            .await?
        {
            if ids.iter().any(|id| key.ends_with(&id.to_string()))
                && self.storage.delete_setting(&key).await?
            {
                deleted += 1;
            }
        }
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use tess_core::memory::MemoryStorage;

    use super::*;
    use crate::FieldKind;

    fn input(slug: &str) -> FormInput {
        FormInput {
            slug: slug.into(),
            title: "Book a table".into(),
            fields: vec![
                FormField {
                    name: "email".into(),
                    label: "Email".into(),
                    kind: FieldKind::Email,
                    required: true,
                    help: None,
                    placeholder: None,
                    options: vec![],
                    max_length: None,
                    min: None,
                    max: None,
                },
                FormField {
                    name: "day".into(),
                    label: "Day".into(),
                    kind: FieldKind::Date,
                    required: true,
                    help: None,
                    placeholder: None,
                    options: vec![],
                    max_length: None,
                    min: None,
                    max: None,
                },
            ],
            enabled: true,
            ..Default::default()
        }
    }

    fn meta(client: &str) -> SubmitMeta {
        SubmitMeta {
            client: client.into(),
            source: Some("https://example.com/book".into()),
        }
    }

    #[tokio::test]
    async fn forms_and_submissions() {
        let svc = FormsService::new(
            Arc::new(MemoryStorage::new()),
            FormsConfig { rate_per_minute: 3 },
        );
        let admin = Actor::system();
        let contributor = Actor {
            id: None,
            name: "c".into(),
            role: Role::Contributor,
            scope: vec![],
        };
        assert!(
            svc.create(input("booking"), &contributor).await.is_err(),
            "admins only"
        );
        let form = svc.create(input("booking"), &admin).await.unwrap();
        assert!(matches!(
            svc.create(input("booking"), &admin).await,
            Err(CmsError::Conflict(_))
        ));

        let ok = json!({"email": "ada@example.com", "day": "2026-12-24"});
        let (_, sub) = svc
            .submit("booking", ok.as_object().unwrap(), meta("1.1.1.1"))
            .await
            .unwrap();
        let sub = sub.unwrap();
        assert_eq!(sub.data["day"], "2026-12-24");
        let bad = json!({"email": "nope"});
        match svc
            .submit("booking", bad.as_object().unwrap(), meta("1.1.1.1"))
            .await
        {
            Err(SubmitError::Invalid(e)) => assert_eq!(e.len(), 2),
            other => panic!("{other:?}"),
        }
        let bot = json!({"email": "bot@example.com", "day": "2026-12-24", "_hp": "http://spam"});
        assert!(
            svc.submit("booking", bot.as_object().unwrap(), meta("1.1.1.1"))
                .await
                .unwrap()
                .1
                .is_none(),
            "honeypot"
        );
        assert!(matches!(
            svc.submit("booking", ok.as_object().unwrap(), meta("1.1.1.1"))
                .await,
            Err(SubmitError::RateLimited)
        ));
        assert!(
            svc.submit("booking", ok.as_object().unwrap(), meta("2.2.2.2"))
                .await
                .is_ok(),
            "per client"
        );
        assert!(matches!(
            svc.submit("nope", ok.as_object().unwrap(), meta("3.3.3.3"))
                .await,
            Err(SubmitError::NotFound)
        ));

        let (items, total) = svc.submissions(form.id, 0, 10, &admin).await.unwrap();
        assert_eq!(
            (total, items.len()),
            (2, 2),
            "the honeypot submission is not stored"
        );
        assert!(
            items[0].submitted_at >= items[1].submitted_at,
            "newest first"
        );
        assert!(svc.submissions(form.id, 0, 10, &contributor).await.is_err());
        assert_eq!(
            svc.delete_submissions(form.id, &[items[0].id], &admin)
                .await
                .unwrap(),
            1
        );
        assert_eq!(svc.submission_count(form.id).await.unwrap(), 1);

        // Disabled forms are not public.
        let mut off = input("booking");
        off.enabled = false;
        svc.update(form.id, off, &admin).await.unwrap();
        assert!(svc.public("booking").await.unwrap().is_none());
        assert!(matches!(
            svc.submit("booking", ok.as_object().unwrap(), meta("4.4.4.4"))
                .await,
            Err(SubmitError::NotFound)
        ));

        svc.delete(form.id, &admin).await.unwrap();
        assert_eq!(
            svc.submission_count(form.id).await.unwrap(),
            0,
            "submissions deleted with the form"
        );
    }
}
