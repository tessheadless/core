// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::{Value, json};
use tess_core::model::{format_ts, now};
use tess_core::secrets::{SecretBox, random_secret};
use tess_core::{Actor, ActorRef, Cms, CmsError, EntryAction, MODEL_EVENTS, Result, Storage};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{DeliveryRecord, DeliveryStatus, SealedHeader, WebhookRecord, signature};

const SETTINGS_PREFIX: &str = "webhooks/";
const LOG_PREFIX: &str = "webhook-log/";
/// Subscriptions changed on other instances are picked up within this delay.
const CACHE_TTL: Duration = Duration::from_secs(10);
const MAX_ERROR_LEN: usize = 500;

static EVENT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\*|[A-Za-z][A-Za-z0-9]*(\.[A-Za-z][A-Za-z0-9]*)*(\.\*)?)$").unwrap());
static HEADER_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9-]{1,64}$").unwrap());
const RESERVED_HEADERS: &[&str] = &["content-type", "content-length", "host", "user-agent", "transfer-encoding"];

/// Encodes an entry id for the API (opaque ids, stable refs).
pub type IdFn = Arc<dyn Fn(&str, Uuid) -> String + Send + Sync>;

#[derive(Debug, Clone)]
pub struct WebhookConfig {
    /// Attempts per delivery, including the first one.
    pub max_attempts: u32,
    /// Waits between attempts (the last value repeats).
    pub retry_delays: Vec<Duration>,
    pub timeout: Duration,
    /// Deliver within the request instead of in the background, with a single
    /// attempt (AWS Lambda freezes background tasks after the response).
    pub inline: bool,
    /// Hosts webhooks may target (`*.example.com` allowed); empty means any.
    pub allowed_hosts: Vec<String>,
    /// Deliveries kept per webhook.
    pub log_size: usize,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        WebhookConfig {
            max_attempts: 5,
            retry_delays: [10, 60, 300, 1800].map(Duration::from_secs).to_vec(),
            timeout: Duration::from_secs(10),
            inline: false,
            allowed_hosts: vec![],
            log_size: 25,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WebhookInput {
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub events: Vec<String>,
    pub content_types: Vec<String>,
    pub headers: Vec<(String, String)>,
    /// Signing secret; generated when `None`.
    pub secret: Option<String>,
    pub managed_by: Option<String>,
}

/// `None` keeps the current value.
#[derive(Debug, Clone, Default)]
pub struct WebhookUpdate {
    pub name: Option<String>,
    pub url: Option<String>,
    pub enabled: Option<bool>,
    pub events: Option<Vec<String>>,
    pub content_types: Option<Vec<String>>,
    /// Replaces all headers.
    pub headers: Option<Vec<(String, String)>>,
}

/// Result of an attempt, before it is recorded.
#[derive(Debug, Clone)]
pub struct DeliveryOutcome {
    pub ok: bool,
    pub retryable: bool,
    pub response_status: Option<u16>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

pub struct WebhookService {
    storage: Arc<dyn Storage>,
    /// Hook-free view of the CMS, to read models when building payloads.
    reader: Cms,
    secrets: SecretBox,
    pub(crate) ids: IdFn,
    pub(crate) refs: IdFn,
    client: reqwest::Client,
    config: WebhookConfig,
    cache: RwLock<Option<(Instant, Arc<Vec<WebhookRecord>>)>>,
    limiter: Arc<Semaphore>,
    extra_events: RwLock<Vec<String>>,
}

fn key(id: &str) -> String {
    format!("{SETTINGS_PREFIX}{id}")
}

fn secret_purpose(id: &str) -> String {
    format!("webhook/{id}/secret")
}

fn header_purpose(id: &str, name: &str) -> String {
    format!("webhook/{id}/header/{}", name.to_ascii_lowercase())
}

fn truncate(mut s: String) -> String {
    if s.len() > MAX_ERROR_LEN {
        let mut end = MAX_ERROR_LEN;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
        s.push('…');
    }
    s
}

fn host_allowed(allowed: &[String], host: &str) -> bool {
    allowed.is_empty()
        || allowed.iter().any(|a| match a.strip_prefix("*.") {
            Some(domain) => host.ends_with(&format!(".{domain}")),
            None => a.eq_ignore_ascii_case(host),
        })
}

impl WebhookService {
    pub fn new(storage: Arc<dyn Storage>, secrets: SecretBox, ids: IdFn, refs: IdFn, config: WebhookConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            // A redirect could point anywhere: receivers must answer directly.
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("Tess-Webhooks/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client");
        WebhookService {
            reader: Cms::new(storage.clone()),
            storage,
            secrets,
            ids,
            refs,
            client,
            config,
            cache: RwLock::new(None),
            limiter: Arc::new(Semaphore::new(16)),
            extra_events: RwLock::new(vec![]),
        }
    }

    pub fn config(&self) -> &WebhookConfig {
        &self.config
    }

    pub(crate) fn reader(&self) -> &Cms {
        &self.reader
    }

    /// Adds events emitted by other services (e.g. translations) to the catalog.
    pub fn register_events(&self, events: &[&str]) {
        let mut all = self.extra_events.write().expect("events lock");
        for e in events {
            if !all.iter().any(|x| x == e) {
                all.push(e.to_string());
            }
        }
    }

    /// Every event a webhook can subscribe to.
    pub fn events(&self) -> Vec<String> {
        let mut all: Vec<String> = EntryAction::ALL.iter().map(|a| a.event_name()).collect();
        all.extend(MODEL_EVENTS.iter().map(|e| e.to_string()));
        all.extend(self.extra_events.read().expect("events lock").iter().cloned());
        all.push("webhook.ping".into());
        all
    }

    // ---- management ----------------------------------------------------

    pub async fn list(&self) -> Result<Vec<WebhookRecord>> {
        let mut all: Vec<WebhookRecord> = self
            .storage
            .list_settings(SETTINGS_PREFIX)
            .await?
            .into_iter()
            .map(|(_, v)| serde_json::from_value(v).map_err(CmsError::storage))
            .collect::<Result<_>>()?;
        all.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
        Ok(all)
    }

    pub async fn get(&self, id: &str) -> Result<WebhookRecord> {
        let v = self.storage.get_setting(&key(id)).await?.ok_or_else(|| CmsError::NotFound(format!("webhook `{id}`")))?;
        serde_json::from_value(v).map_err(CmsError::storage)
    }

    async fn save(&self, record: &WebhookRecord) -> Result<()> {
        let v = serde_json::to_value(record).map_err(CmsError::storage)?;
        self.storage.put_setting(&key(&record.id), &v).await?;
        self.invalidate();
        Ok(())
    }

    fn invalidate(&self) {
        *self.cache.write().expect("cache lock") = None;
    }

    fn check_url(&self, url: &str) -> Result<()> {
        let parsed = reqwest::Url::parse(url).map_err(|_| CmsError::validation(format!("`{url}` is not a valid URL")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(CmsError::validation("webhook URLs must use http or https"));
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(CmsError::validation("put credentials in a header, not in the URL"));
        }
        let host = parsed.host_str().ok_or_else(|| CmsError::validation("the webhook URL has no host"))?;
        if !host_allowed(&self.config.allowed_hosts, host) {
            return Err(CmsError::validation(format!("`{host}` is not in TESS_WEBHOOK_ALLOWED_HOSTS")));
        }
        Ok(())
    }

    fn check_fields(name: &str, events: &[String]) -> Result<()> {
        if name.trim().is_empty() || name.chars().count() > 100 {
            return Err(CmsError::validation("the webhook name must have 1 to 100 characters"));
        }
        if events.is_empty() {
            return Err(CmsError::validation("subscribe the webhook to at least one event"));
        }
        if let Some(bad) = events.iter().find(|e| !EVENT_PATTERN.is_match(e)) {
            return Err(CmsError::validation(format!("invalid event pattern `{bad}`")));
        }
        Ok(())
    }

    fn seal_headers(&self, id: &str, headers: Vec<(String, String)>) -> Result<Vec<SealedHeader>> {
        let mut seen = std::collections::HashSet::new();
        headers
            .into_iter()
            .map(|(name, value)| {
                let lower = name.to_ascii_lowercase();
                if !HEADER_NAME.is_match(&name) || RESERVED_HEADERS.contains(&lower.as_str()) || lower.starts_with("x-tess-")
                {
                    return Err(CmsError::validation(format!("header `{name}` cannot be set")));
                }
                if !seen.insert(lower) {
                    return Err(CmsError::validation(format!("header `{name}` is given twice")));
                }
                if value.chars().any(|c| c.is_control()) {
                    return Err(CmsError::validation(format!("the value of `{name}` contains control characters")));
                }
                Ok(SealedHeader { value: self.secrets.seal(&header_purpose(id, &name), &value), name })
            })
            .collect()
    }

    /// Returns the webhook and its signing secret (shown once).
    pub async fn create(&self, input: WebhookInput, actor: &Actor) -> Result<(WebhookRecord, String)> {
        actor.require_admin()?;
        Self::check_fields(&input.name, &input.events)?;
        self.check_url(&input.url)?;
        let secret = match input.secret {
            Some(s) if s.len() < 16 => return Err(CmsError::validation("the signing secret must have at least 16 characters")),
            Some(s) => s,
            None => random_secret(),
        };
        let id = Uuid::new_v4().to_string();
        let ts = format_ts(now());
        let record = WebhookRecord {
            headers: self.seal_headers(&id, input.headers)?,
            secret: self.secrets.seal(&secret_purpose(&id), &secret),
            id,
            name: input.name.trim().to_string(),
            url: input.url,
            enabled: input.enabled,
            events: input.events,
            content_types: input.content_types,
            managed_by: input.managed_by.filter(|m| !m.trim().is_empty()),
            created_by: actor.name.clone(),
            created_at: ts.clone(),
            updated_at: ts,
        };
        self.save(&record).await?;
        Ok((record, secret))
    }

    pub async fn update(&self, id: &str, update: WebhookUpdate, actor: &Actor) -> Result<WebhookRecord> {
        actor.require_admin()?;
        let mut r = self.get(id).await?;
        if let Some(name) = update.name {
            r.name = name.trim().to_string();
        }
        if let Some(url) = update.url {
            self.check_url(&url)?;
            r.url = url;
        }
        if let Some(enabled) = update.enabled {
            r.enabled = enabled;
        }
        if let Some(events) = update.events {
            r.events = events;
        }
        if let Some(cts) = update.content_types {
            r.content_types = cts;
        }
        if let Some(headers) = update.headers {
            r.headers = self.seal_headers(id, headers)?;
        }
        Self::check_fields(&r.name, &r.events)?;
        r.updated_at = format_ts(now());
        self.save(&r).await?;
        Ok(r)
    }

    pub async fn rotate_secret(&self, id: &str, actor: &Actor) -> Result<String> {
        actor.require_admin()?;
        let mut r = self.get(id).await?;
        let secret = random_secret();
        r.secret = self.secrets.seal(&secret_purpose(id), &secret);
        r.updated_at = format_ts(now());
        self.save(&r).await?;
        Ok(secret)
    }

    pub async fn delete(&self, id: &str, actor: &Actor) -> Result<()> {
        actor.require_admin()?;
        if !self.storage.delete_setting(&key(id)).await? {
            return Err(CmsError::NotFound(format!("webhook `{id}`")));
        }
        for (k, _) in self.storage.list_settings(&format!("{LOG_PREFIX}{id}/")).await? {
            self.storage.delete_setting(&k).await?;
        }
        self.invalidate();
        Ok(())
    }

    /// Most recent first.
    pub async fn deliveries(&self, id: &str, limit: usize) -> Result<Vec<DeliveryRecord>> {
        let mut all: Vec<DeliveryRecord> = self
            .storage
            .list_settings(&format!("{LOG_PREFIX}{id}/"))
            .await?
            .into_iter()
            .filter_map(|(_, v)| serde_json::from_value(v).ok())
            .collect();
        all.reverse();
        all.truncate(limit);
        Ok(all)
    }

    /// Sends a `webhook.ping` event right away and returns the outcome.
    pub async fn test(&self, id: &str, actor: &Actor) -> Result<DeliveryRecord> {
        actor.require_admin()?;
        let hook = self.get(id).await?;
        let data = json!({ "message": "Test delivery from Tess", "webhook": { "id": hook.id, "name": hook.name } });
        let delivery = self.envelope("webhook.ping", None, data, &actor.reference());
        Ok(self.deliver(&hook, delivery, 1).await)
    }

    // ---- delivery ------------------------------------------------------

    /// Enabled webhooks, cached for [`CACHE_TTL`].
    pub(crate) async fn subscriptions(&self) -> Arc<Vec<WebhookRecord>> {
        if let Some((at, hooks)) = self.cache.read().expect("cache lock").as_ref()
            && at.elapsed() < CACHE_TTL
        {
            return hooks.clone();
        }
        let hooks = match self.list().await {
            Ok(all) => Arc::new(all.into_iter().filter(|h| h.enabled).collect::<Vec<_>>()),
            Err(e) => {
                tracing::error!(error = %e, "cannot load webhooks");
                Arc::new(vec![])
            }
        };
        *self.cache.write().expect("cache lock") = Some((Instant::now(), hooks.clone()));
        hooks
    }

    pub(crate) async fn matching(&self, event: &str, content_type: Option<&str>) -> Vec<WebhookRecord> {
        self.subscriptions().await.iter().filter(|h| h.subscribes_to(event, content_type)).cloned().collect()
    }

    pub(crate) fn envelope(&self, event: &str, content_type: Option<&str>, data: Value, actor: &ActorRef) -> Value {
        let mut v = json!({
            "id": Uuid::new_v4().to_string(),
            "event": event,
            "createdAt": format_ts(now()),
            "actor": { "name": actor.name },
            "data": data,
        });
        if let Some(ct) = content_type {
            v["contentType"] = json!(ct);
        }
        v
    }

    /// Sends `delivery` to every webhook in `hooks`: in the background with
    /// retries, or inline with one attempt (see [`WebhookConfig::inline`]).
    pub(crate) async fn dispatch(self: &Arc<Self>, hooks: Vec<WebhookRecord>, delivery: Value) {
        for hook in hooks {
            let mut delivery = delivery.clone();
            delivery["id"] = json!(Uuid::new_v4().to_string());
            if self.config.inline {
                self.deliver(&hook, delivery, 1).await;
            } else {
                let this = self.clone();
                let attempts = self.config.max_attempts.max(1);
                tokio::spawn(async move {
                    this.deliver(&hook, delivery, attempts).await;
                });
            }
        }
    }

    async fn deliver(&self, hook: &WebhookRecord, delivery: Value, max_attempts: u32) -> DeliveryRecord {
        let body = serde_json::to_vec(&delivery).unwrap_or_default();
        let created = now();
        let mut record = DeliveryRecord {
            id: delivery["id"].as_str().unwrap_or_default().to_string(),
            webhook_id: hook.id.clone(),
            event: delivery["event"].as_str().unwrap_or_default().to_string(),
            status: DeliveryStatus::Retrying,
            attempts: 0,
            response_status: None,
            error: None,
            duration_ms: 0,
            created_at: format_ts(created),
            updated_at: format_ts(created),
        };
        let log_key = format!("{LOG_PREFIX}{}/{:013}-{}", hook.id, created.timestamp_millis(), record.id);
        for attempt in 1..=max_attempts {
            let outcome = self.send_once(hook, &record.id, &record.event, &body).await;
            record.attempts = attempt;
            record.response_status = outcome.response_status;
            record.error = outcome.error.clone();
            record.duration_ms = outcome.duration_ms;
            record.updated_at = format_ts(now());
            let last = outcome.ok || !outcome.retryable || attempt == max_attempts;
            record.status = match (outcome.ok, last) {
                (true, _) => DeliveryStatus::Success,
                (false, true) => DeliveryStatus::Failed,
                (false, false) => DeliveryStatus::Retrying,
            };
            self.log(&log_key, &record, attempt == 1).await;
            if last {
                if !outcome.ok {
                    tracing::warn!(webhook = %hook.name, event = %record.event, error = ?record.error, "webhook delivery failed");
                }
                break;
            }
            let delays = &self.config.retry_delays;
            let wait = delays.get(attempt as usize - 1).or(delays.last()).copied().unwrap_or(Duration::from_secs(30));
            tokio::time::sleep(wait).await;
        }
        record
    }

    async fn send_once(&self, hook: &WebhookRecord, delivery_id: &str, event: &str, body: &[u8]) -> DeliveryOutcome {
        let started = Instant::now();
        let fail = |error: String, retryable: bool| DeliveryOutcome {
            ok: false,
            retryable,
            response_status: None,
            error: Some(truncate(error)),
            duration_ms: started.elapsed().as_millis() as u64,
        };
        let secret = match self.secrets.open(&secret_purpose(&hook.id), &hook.secret) {
            Ok(s) => s,
            Err(e) => return fail(e.to_string(), false),
        };
        let _permit = self.limiter.acquire().await;
        let ts = now().timestamp();
        let mut req = self
            .client
            .post(&hook.url)
            .header("Content-Type", "application/json")
            .header("X-Tess-Event", event)
            .header("X-Tess-Delivery", delivery_id)
            .header("X-Tess-Webhook", &hook.id)
            .header("X-Tess-Signature", signature(&secret, ts, body));
        for h in &hook.headers {
            match self.secrets.open(&header_purpose(&hook.id, &h.name), &h.value) {
                Ok(v) => req = req.header(&h.name, v),
                Err(e) => return fail(e.to_string(), false),
            }
        }
        match req.body(body.to_vec()).send().await {
            Ok(res) => {
                let status = res.status();
                let text = if status.is_success() { String::new() } else { res.text().await.unwrap_or_default() };
                let code = status.as_u16();
                DeliveryOutcome {
                    ok: status.is_success(),
                    retryable: status.is_server_error() || code == 408 || code == 429,
                    response_status: Some(code),
                    error: (!status.is_success()).then(|| truncate(format!("HTTP {code}: {text}"))),
                    duration_ms: started.elapsed().as_millis() as u64,
                }
            }
            Err(e) => {
                let mut msg = e.to_string();
                let mut source = std::error::Error::source(&e);
                while let Some(s) = source {
                    msg.push_str(": ");
                    msg.push_str(&s.to_string());
                    source = s.source();
                }
                fail(msg, true)
            }
        }
    }

    /// Saves a delivery and, for new ones, drops the oldest beyond `log_size`.
    async fn log(&self, key: &str, record: &DeliveryRecord, is_new: bool) {
        let v = serde_json::to_value(record).unwrap_or_default();
        if let Err(e) = self.storage.put_setting(key, &v).await {
            tracing::error!(error = %e, "cannot record a webhook delivery");
            return;
        }
        if !is_new {
            return;
        }
        let prefix = format!("{LOG_PREFIX}{}/", record.webhook_id);
        match self.storage.list_settings(&prefix).await {
            Ok(all) if all.len() > self.config.log_size => {
                for (k, _) in &all[..all.len() - self.config.log_size] {
                    let _ = self.storage.delete_setting(k).await;
                }
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "cannot prune the webhook log"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        let allowed = vec!["n8n".to_string(), "*.example.com".to_string()];
        assert!(host_allowed(&[], "anything"));
        assert!(host_allowed(&allowed, "n8n"));
        assert!(host_allowed(&allowed, "hooks.example.com"));
        assert!(!host_allowed(&allowed, "example.com.evil.io"));
        assert!(!host_allowed(&allowed, "evil.io"));
    }
}
