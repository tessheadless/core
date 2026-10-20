// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Extension points of the content lifecycle.
//!
//! Built-in features (search indexing, ...) and admin-installed plugins
//! (WebAssembly modules, see the `tess-plugins` crate) all implement [`Hook`]
//! and are registered on [`crate::Cms`] through a [`HookRegistry`], which can
//! be swapped at runtime when plugins are enabled or disabled.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::access::{Actor, ActorRef};
use crate::error::Result;
use crate::model::{ContentType, Entry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryAction {
    Create,
    Update,
    Delete,
    Publish,
    Unpublish,
    /// Workflow: sent for review.
    Submit,
    /// Workflow: reviewer approved; the entry is published.
    Approve,
    /// Workflow: reviewer sent the entry back with a comment.
    RequestChanges,
}

impl EntryAction {
    pub const ALL: [EntryAction; 8] = [
        EntryAction::Create,
        EntryAction::Update,
        EntryAction::Delete,
        EntryAction::Publish,
        EntryAction::Unpublish,
        EntryAction::Submit,
        EntryAction::Approve,
        EntryAction::RequestChanges,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            EntryAction::Create => "create",
            EntryAction::Update => "update",
            EntryAction::Delete => "delete",
            EntryAction::Publish => "publish",
            EntryAction::Unpublish => "unpublish",
            EntryAction::Submit => "submit",
            EntryAction::Approve => "approve",
            EntryAction::RequestChanges => "requestChanges",
        }
    }

    /// Public event name, e.g. `entry.publish` (webhooks, workflows).
    pub fn event_name(self) -> String {
        format!("entry.{}", self.as_str())
    }
}

/// Emitted after an entry change has been persisted.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryEvent {
    pub action: EntryAction,
    pub content_type: String,
    /// State after the change (`None` after a delete).
    pub entry: Option<Entry>,
    /// State before the change (`None` after a create).
    pub previous: Option<Entry>,
    /// Who made the change.
    pub actor: ActorRef,
}

/// A change outside entries: content models (`contentType.create`,
/// `contentType.update`, `contentType.delete`), components (`component.*`)
/// and built-in services such as translations (`i18n.*`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemEvent {
    /// Dotted name, `<subject>.<action>`.
    pub event: String,
    /// Content type concerned, if any (used by per-type filters).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Event payload, e.g. the content type definition after the change.
    pub data: Value,
    pub actor: ActorRef,
}

impl SystemEvent {
    pub fn new(event: impl Into<String>, data: Value, actor: &Actor) -> Self {
        SystemEvent { event: event.into(), content_type: None, data, actor: actor.reference() }
    }

    pub fn for_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }
}

/// Names of the system events emitted by the core (services add their own).
pub const MODEL_EVENTS: &[&str] = &[
    "contentType.create",
    "contentType.update",
    "contentType.delete",
    "component.create",
    "component.update",
    "component.delete",
];

#[async_trait]
pub trait Hook: Send + Sync + 'static {
    /// Unique name, used in logs and errors.
    fn name(&self) -> &str;

    /// Runs before a write is persisted, with the normalized draft data. For
    /// create/update/publish/approve it may modify `data` (the result is
    /// validated again); for other actions modifications are ignored. It can
    /// reject the write by returning an error, typically
    /// [`crate::CmsError::Validation`].
    async fn before_write(
        &self,
        _action: EntryAction,
        _content_type: &ContentType,
        _data: &mut Map<String, Value>,
        _actor: &Actor,
    ) -> Result<()> {
        Ok(())
    }

    /// Runs after a change has been persisted. Cannot fail the request;
    /// errors must be handled (logged) by the hook itself.
    async fn after_write(&self, _event: &EntryEvent) {}

    /// Runs after a content type was created/updated (`deleted == false`) or deleted.
    async fn after_content_type_change(&self, _content_type: &ContentType, _deleted: bool) {}

    /// Runs after a [`SystemEvent`]. Like `after_write`, it cannot fail the request.
    async fn on_event(&self, _event: &SystemEvent) {}
}

/// Shared, swappable list of hooks, run in order.
#[derive(Clone, Default)]
pub struct HookRegistry {
    hooks: Arc<RwLock<Vec<Arc<dyn Hook>>>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&self, hook: Arc<dyn Hook>) {
        self.hooks.write().expect("hook registry poisoned").push(hook);
    }

    /// Replaces all hooks whose name starts with `prefix` by `hooks`
    /// (e.g. every `plugin:` hook when the plugin set changes).
    pub fn replace_prefixed(&self, prefix: &str, hooks: Vec<Arc<dyn Hook>>) {
        let mut all = self.hooks.write().expect("hook registry poisoned");
        all.retain(|h| !h.name().starts_with(prefix));
        all.extend(hooks);
    }

    /// Snapshot of the current hooks (no lock held while they run).
    pub fn snapshot(&self) -> Vec<Arc<dyn Hook>> {
        self.hooks.read().expect("hook registry poisoned").clone()
    }

    pub fn names(&self) -> Vec<String> {
        self.snapshot().iter().map(|h| h.name().to_string()).collect()
    }

    /// Delivers a [`SystemEvent`] to every hook.
    pub async fn notify(&self, event: &SystemEvent) {
        for hook in self.snapshot() {
            hook.on_event(event).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;
    use crate::memory::MemoryStorage;
    use crate::model::{ContentTypeDef, FieldDef, FieldType};
    use crate::{Cms, CmsError};

    /// Upper-cases titles, rejects "forbidden" ones, records events.
    struct Upper(Mutex<Vec<EntryAction>>);

    #[async_trait]
    impl Hook for Upper {
        fn name(&self) -> &str {
            "test:upper"
        }

        async fn before_write(
            &self,
            action: EntryAction,
            _: &ContentType,
            data: &mut Map<String, Value>,
            _: &Actor,
        ) -> Result<()> {
            if action == EntryAction::Delete {
                return Ok(());
            }
            let title = data.get("title").and_then(Value::as_str).unwrap_or_default().to_string();
            if title == "forbidden" {
                return Err(CmsError::validation("title is forbidden"));
            }
            data.insert("title".into(), json!(title.to_uppercase()));
            Ok(())
        }

        async fn after_write(&self, event: &EntryEvent) {
            self.0.lock().unwrap().push(event.action);
        }
    }

    #[tokio::test]
    async fn hooks_transform_reject_and_observe() {
        let cms = Cms::new(Arc::new(MemoryStorage::new()));
        let hook = Arc::new(Upper(Mutex::new(vec![])));
        cms.hooks().add(hook.clone());
        cms.create_content_type(ContentTypeDef { workflow: false,
            api_id: "Note".into(),
            plural_api_id: None,
            description: None,
            fields: vec![FieldDef { name: "title".into(), field_type: FieldType::text(), required: true, description: None }],
        }, &Actor::system())
        .await
        .unwrap();
        let data = |t: &str| json!({ "title": t }).as_object().unwrap().clone();
        let e = cms.create_entry("Note", data("hi"), &Actor::system()).await.unwrap();
        assert_eq!(e.draft["title"], "HI");
        assert!(matches!(cms.create_entry("Note", data("forbidden"), &Actor::system()).await, Err(CmsError::Validation(_))));
        cms.publish_entry("Note", e.id, &Actor::system()).await.unwrap();
        cms.delete_entry("Note", e.id, &Actor::system()).await.unwrap();
        assert_eq!(*hook.0.lock().unwrap(), vec![EntryAction::Create, EntryAction::Publish, EntryAction::Delete]);

        cms.hooks().replace_prefixed("test:", vec![]);
        let e = cms.create_entry("Note", data("forbidden"), &Actor::system()).await.unwrap();
        assert_eq!(e.draft["title"], "forbidden");
    }
}
