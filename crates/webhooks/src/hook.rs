// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use tess_core::model::format_ts;
use tess_core::redact::redact_secrets;
use tess_core::validate::Components;
use tess_core::{ContentType, Entry, EntryEvent, Hook, SystemEvent};

use crate::service::WebhookService;

/// Registered on the CMS as hook `webhooks`.
pub struct WebhookHook {
    service: Arc<WebhookService>,
}

impl WebhookHook {
    pub fn new(service: Arc<WebhookService>) -> Self {
        WebhookHook { service }
    }

    fn entry_json(&self, ct: Option<&ContentType>, components: &Components, entry: &Entry) -> Value {
        let clean = |data: &Map<String, Value>| {
            let mut data = data.clone();
            match ct {
                Some(ct) => redact_secrets(&ct.fields, &mut data, components),
                // Model unknown (deleted meanwhile): send nothing rather than secrets.
                None => data.clear(),
            }
            Value::Object(data)
        };
        let s = &self.service;
        json!({
            "id": (s.ids)(&entry.content_type, entry.id),
            "ref": (s.refs)(&entry.content_type, entry.id),
            "draft": clean(&entry.draft),
            "published": entry.published.as_ref().map(clean),
            "createdAt": format_ts(entry.created_at),
            "updatedAt": format_ts(entry.updated_at),
            "publishedAt": entry.published_at.map(format_ts),
            "workflowStatus": entry.status.as_str(),
            "createdBy": entry.created_by.as_ref().map(|a| &a.name),
            "updatedBy": entry.updated_by.as_ref().map(|a| &a.name),
        })
    }
}

#[async_trait]
impl Hook for WebhookHook {
    fn name(&self) -> &str {
        "webhooks"
    }

    async fn after_write(&self, event: &EntryEvent) {
        let name = event.action.event_name();
        let hooks = self.service.matching(&name, Some(&event.content_type)).await;
        if hooks.is_empty() {
            return;
        }
        let reader = self.service.reader();
        let ct = reader.content_type(&event.content_type).await.ok();
        let components = reader.components().await.unwrap_or_default();
        let entry = |e: &Option<Entry>| e.as_ref().map(|e| self.entry_json(ct.as_ref(), &components, e));
        let data = json!({ "entry": entry(&event.entry), "previous": entry(&event.previous) });
        let delivery = self.service.envelope(&name, Some(&event.content_type), data, &event.actor);
        self.service.dispatch(hooks, delivery).await;
    }

    async fn on_event(&self, event: &SystemEvent) {
        let hooks = self.service.matching(&event.event, event.content_type.as_deref()).await;
        if hooks.is_empty() {
            return;
        }
        let delivery =
            self.service.envelope(&event.event, event.content_type.as_deref(), event.data.clone(), &event.actor);
        self.service.dispatch(hooks, delivery).await;
    }
}
