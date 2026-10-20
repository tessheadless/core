// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::sync::Arc;

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::access::Actor;
use crate::error::{CmsError, Result};
use crate::hooks::{EntryAction, EntryEvent, HookRegistry, SystemEvent};
use crate::model::{
    Component, ComponentDef, ContentType, ContentTypeDef, Entry, FieldType, HistoryAction, Stage,
    WorkflowStatus, now,
};
use crate::query::{EntryQuery, Page};
use crate::storage::Storage;
use crate::validate::{
    Components, normalize_entry_data, relation_refs, validate_component, validate_content_type,
};

/// Extra check run on the full set of content types before a schema change is
/// saved. The GraphQL layer uses it to reject models whose generated type
/// names would collide.
pub type SchemaGuard = Arc<dyn Fn(&[ContentType], &Components) -> Result<()> + Send + Sync>;

const COMPONENTS_PREFIX: &str = "components/";

/// Business logic shared by every API surface and storage backend. Every
/// write takes the [`Actor`] performing it; permissions (see
/// [`crate::access`]) and the review workflow are enforced here.
#[derive(Clone)]
pub struct Cms {
    storage: Arc<dyn Storage>,
    guard: Option<SchemaGuard>,
    hooks: HookRegistry,
}

impl Cms {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Cms {
            storage,
            guard: None,
            hooks: HookRegistry::new(),
        }
    }

    pub fn with_schema_guard(mut self, guard: SchemaGuard) -> Self {
        self.guard = Some(guard);
        self
    }

    pub fn hooks(&self) -> &HookRegistry {
        &self.hooks
    }

    pub fn storage(&self) -> &Arc<dyn Storage> {
        &self.storage
    }

    // ---- content types -------------------------------------------------

    /// All content types, sorted by apiId.
    pub async fn content_types(&self) -> Result<Vec<ContentType>> {
        let mut all = self.storage.list_content_types().await?;
        all.sort_by(|a, b| a.api_id.cmp(&b.api_id));
        Ok(all)
    }

    pub async fn content_type(&self, api_id: &str) -> Result<ContentType> {
        self.storage
            .get_content_type(api_id)
            .await?
            .ok_or_else(|| CmsError::NotFound(format!("content type `{api_id}`")))
    }

    pub async fn create_content_type(
        &self,
        def: ContentTypeDef,
        actor: &Actor,
    ) -> Result<ContentType> {
        actor.require_admin()?;
        let all = self.content_types().await?;
        if all.iter().any(|c| c.api_id == def.api_id) {
            return Err(CmsError::Conflict(format!(
                "content type `{}` already exists",
                def.api_id
            )));
        }
        let components = self.components().await?;
        let plural_api_id = validate_content_type(&def, &all, &components)?;
        let ts = now();
        let ct = ContentType {
            api_id: def.api_id,
            plural_api_id,
            description: def.description,
            fields: def.fields,
            workflow: def.workflow,
            created_at: ts,
            updated_at: ts,
        };
        self.check_guard(all, &ct, &components)?;
        self.storage.put_content_type(&ct).await?;
        self.content_type_changed(&ct, "create", actor).await;
        Ok(ct)
    }

    /// Replaces the definition of a content type. Existing entries are kept;
    /// data of removed fields is ignored and dropped on the next write.
    pub async fn update_content_type(
        &self,
        api_id: &str,
        def: ContentTypeDef,
        actor: &Actor,
    ) -> Result<ContentType> {
        actor.require_admin()?;
        if def.api_id != api_id {
            return Err(CmsError::validation(
                "renaming a content type is not supported",
            ));
        }
        let existing = self.content_type(api_id).await?;
        let others: Vec<_> = self
            .content_types()
            .await?
            .into_iter()
            .filter(|c| c.api_id != api_id)
            .collect();
        let components = self.components().await?;
        let plural_api_id = validate_content_type(&def, &others, &components)?;
        let ct = ContentType {
            api_id: def.api_id,
            plural_api_id,
            description: def.description,
            fields: def.fields,
            workflow: def.workflow,
            created_at: existing.created_at,
            updated_at: now(),
        };
        self.check_guard(others, &ct, &components)?;
        self.storage.put_content_type(&ct).await?;
        self.content_type_changed(&ct, "update", actor).await;
        Ok(ct)
    }

    /// Deletes a content type and all its entries. Refused while other
    /// content types still have relations pointing to it.
    pub async fn delete_content_type(&self, api_id: &str, actor: &Actor) -> Result<()> {
        actor.require_admin()?;
        let all = self.content_types().await?;
        let Some(deleted) = all.iter().find(|c| c.api_id == api_id).cloned() else {
            return Err(CmsError::NotFound(format!("content type `{api_id}`")));
        };
        for other in all.iter().filter(|c| c.api_id != api_id) {
            for field in &other.fields {
                if matches!(&field.field_type, FieldType::Relation { target, .. } if target == api_id)
                {
                    return Err(CmsError::Conflict(format!(
                        "`{}.{}` still references `{api_id}`",
                        other.api_id, field.name
                    )));
                }
            }
        }
        self.storage.delete_content_type(api_id).await?;
        self.content_type_changed(&deleted, "delete", actor).await;
        Ok(())
    }

    async fn content_type_changed(&self, ct: &ContentType, action: &str, actor: &Actor) {
        let deleted = action == "delete";
        for hook in self.hooks.snapshot() {
            hook.after_content_type_change(ct, deleted).await;
        }
        let data = serde_json::to_value(ct).unwrap_or_default();
        let event =
            SystemEvent::new(format!("contentType.{action}"), data, actor).for_type(&ct.api_id);
        self.hooks.notify(&event).await;
    }

    async fn component_changed(&self, c: &Component, action: &str, actor: &Actor) {
        let data = serde_json::to_value(c).unwrap_or_default();
        self.hooks
            .notify(&SystemEvent::new(
                format!("component.{action}"),
                data,
                actor,
            ))
            .await;
    }

    fn check_guard(
        &self,
        mut others: Vec<ContentType>,
        candidate: &ContentType,
        components: &Components,
    ) -> Result<()> {
        if let Some(guard) = &self.guard {
            others.push(candidate.clone());
            others.sort_by(|a, b| a.api_id.cmp(&b.api_id));
            guard(&others, components)?;
        }
        Ok(())
    }

    // ---- components ----------------------------------------------------

    /// All components, by apiId.
    pub async fn components(&self) -> Result<Components> {
        self.storage
            .list_settings(COMPONENTS_PREFIX)
            .await?
            .into_iter()
            .map(|(_, v)| {
                let c: Component = serde_json::from_value(v).map_err(CmsError::storage)?;
                Ok((c.api_id.clone(), c))
            })
            .collect()
    }

    async fn save_component(&self, c: &Component) -> Result<()> {
        let v = serde_json::to_value(c).map_err(CmsError::storage)?;
        self.storage
            .put_setting(&format!("{COMPONENTS_PREFIX}{}", c.api_id), &v)
            .await
    }

    pub async fn create_component(&self, def: ComponentDef, actor: &Actor) -> Result<Component> {
        actor.require_admin()?;
        let mut components = self.components().await?;
        if components.contains_key(&def.api_id) {
            return Err(CmsError::Conflict(format!(
                "component `{}` already exists",
                def.api_id
            )));
        }
        let types = self.content_types().await?;
        validate_component(&def, &components, &types)?;
        let ts = now();
        let c = Component {
            api_id: def.api_id,
            description: def.description,
            fields: def.fields,
            created_at: ts,
            updated_at: ts,
        };
        components.insert(c.api_id.clone(), c.clone());
        if let Some(guard) = &self.guard {
            guard(&types, &components)?;
        }
        self.save_component(&c).await?;
        self.component_changed(&c, "create", actor).await;
        Ok(c)
    }

    /// Replaces a component's definition; every content type using it changes
    /// accordingly. Existing data of removed fields is ignored.
    pub async fn update_component(
        &self,
        api_id: &str,
        def: ComponentDef,
        actor: &Actor,
    ) -> Result<Component> {
        actor.require_admin()?;
        if def.api_id != api_id {
            return Err(CmsError::validation(
                "renaming a component is not supported",
            ));
        }
        let mut components = self.components().await?;
        let existing = components
            .remove(api_id)
            .ok_or_else(|| CmsError::NotFound(format!("component `{api_id}`")))?;
        let types = self.content_types().await?;
        validate_component(&def, &components, &types)?;
        let c = Component {
            api_id: def.api_id,
            description: def.description,
            fields: def.fields,
            created_at: existing.created_at,
            updated_at: now(),
        };
        components.insert(c.api_id.clone(), c.clone());
        if let Some(guard) = &self.guard {
            guard(&types, &components)?;
        }
        self.save_component(&c).await?;
        self.component_changed(&c, "update", actor).await;
        Ok(c)
    }

    /// Refused while content types or other components still use it.
    pub async fn delete_component(&self, api_id: &str, actor: &Actor) -> Result<()> {
        actor.require_admin()?;
        let components = self.components().await?;
        let Some(removed) = components.get(api_id).cloned() else {
            return Err(CmsError::NotFound(format!("component `{api_id}`")));
        };
        let uses = |fields: &[crate::model::FieldDef]| {
            fields.iter().any(|f| match &f.field_type {
                FieldType::Component { component, .. } => component == api_id,
                FieldType::DynamicZone { components } => components.iter().any(|c| c == api_id),
                _ => false,
            })
        };
        let mut users: Vec<String> = self
            .content_types()
            .await?
            .iter()
            .filter(|t| uses(&t.fields))
            .map(|t| t.api_id.clone())
            .collect();
        users.extend(
            components
                .values()
                .filter(|c| c.api_id != api_id && uses(&c.fields))
                .map(|c| c.api_id.clone()),
        );
        if !users.is_empty() {
            return Err(CmsError::Conflict(format!(
                "`{api_id}` is still used by {}",
                users.join(", ")
            )));
        }
        self.storage
            .delete_setting(&format!("{COMPONENTS_PREFIX}{api_id}"))
            .await?;
        self.component_changed(&removed, "delete", actor).await;
        Ok(())
    }

    // ---- hooks ---------------------------------------------------------

    /// Runs `before_write` hooks. For actions that save data, the result is
    /// validated again and returned; otherwise modifications are ignored.
    async fn run_before_hooks(
        &self,
        action: EntryAction,
        ct: &ContentType,
        data: Map<String, Value>,
        actor: &Actor,
        components: &Components,
    ) -> Result<Map<String, Value>> {
        let hooks = self.hooks.snapshot();
        if hooks.is_empty() {
            return Ok(data);
        }
        let saves_data = matches!(
            action,
            EntryAction::Create | EntryAction::Update | EntryAction::Publish | EntryAction::Approve
        );
        let original = data.clone();
        let mut data = data;
        for hook in hooks {
            hook.before_write(action, ct, &mut data, actor)
                .await
                .map_err(|e| match e {
                    CmsError::Storage(m) => {
                        CmsError::Storage(format!("hook `{}`: {m}", hook.name()))
                    }
                    other => other,
                })?;
        }
        if saves_data {
            normalize_entry_data(ct, data, components)
        } else {
            Ok(original)
        }
    }

    async fn emit(
        &self,
        action: EntryAction,
        entry: Option<Entry>,
        previous: Option<Entry>,
        actor: &Actor,
    ) {
        let hooks = self.hooks.snapshot();
        if hooks.is_empty() {
            return;
        }
        let content_type = entry
            .as_ref()
            .or(previous.as_ref())
            .map(|e| e.content_type.clone())
            .unwrap_or_default();
        let event = EntryEvent {
            action,
            content_type,
            entry,
            previous,
            actor: actor.reference(),
        };
        for hook in hooks {
            hook.after_write(&event).await;
        }
    }

    // ---- entries -------------------------------------------------------

    pub async fn get_entry(
        &self,
        content_type: &str,
        id: Uuid,
        stage: Stage,
    ) -> Result<Option<Entry>> {
        let entry = self.storage.get_entry(content_type, id).await?;
        Ok(entry.filter(|e| e.data(stage).is_some()))
    }

    pub async fn query_entries(&self, content_type: &str, query: &EntryQuery) -> Result<Page> {
        self.content_type(content_type).await?;
        self.storage.query_entries(content_type, query).await
    }

    pub async fn create_entry(
        &self,
        content_type: &str,
        data: Map<String, Value>,
        actor: &Actor,
    ) -> Result<Entry> {
        let ct = self.content_type(content_type).await?;
        actor.require_edit(&ct)?;
        let components = self.components().await?;
        let mut data = prepare_input(&ct, data).await?;
        // UUID fields left empty are generated.
        for f in &ct.fields {
            if matches!(f.field_type, FieldType::Uuid)
                && data.get(&f.name).is_none_or(Value::is_null)
            {
                data.insert(f.name.clone(), Value::String(Uuid::new_v4().to_string()));
            }
        }
        let draft = normalize_entry_data(&ct, data, &components)?;
        let draft = self
            .run_before_hooks(EntryAction::Create, &ct, draft, actor, &components)
            .await?;
        self.check_references(&ct, &draft, &components).await?;
        let ts = now();
        let mut entry = Entry {
            id: Uuid::new_v4(),
            content_type: ct.api_id,
            draft,
            published: None,
            created_at: ts,
            updated_at: ts,
            published_at: None,
            status: WorkflowStatus::Draft,
            created_by: Some(actor.reference()),
            updated_by: Some(actor.reference()),
            history: vec![],
        };
        entry.record(HistoryAction::Created, actor.reference(), None);
        self.storage.put_entry(&entry).await?;
        self.emit(EntryAction::Create, Some(entry.clone()), None, actor)
            .await;
        Ok(entry)
    }

    /// Merges `patch` into the draft. A `null` value clears a field. Editing
    /// sets the entry back to `DRAFT` (also when it was in review, so
    /// reviewers always approve exactly what gets published).
    pub async fn update_entry(
        &self,
        content_type: &str,
        id: Uuid,
        patch: Map<String, Value>,
        actor: &Actor,
    ) -> Result<Entry> {
        let ct = self.content_type(content_type).await?;
        actor.require_edit(&ct)?;
        let mut entry = self.require_entry(content_type, id).await?;
        let previous = entry.clone();
        let components = self.components().await?;
        let mut merged = current_fields(&ct, &entry.draft);
        merged.extend(prepare_input(&ct, patch).await?);
        let draft = normalize_entry_data(&ct, merged, &components)?;
        entry.draft = self
            .run_before_hooks(EntryAction::Update, &ct, draft, actor, &components)
            .await?;
        self.check_references(&ct, &entry.draft, &components)
            .await?;
        entry.updated_at = now();
        entry.updated_by = Some(actor.reference());
        entry.status = WorkflowStatus::Draft;
        entry.record(HistoryAction::Updated, actor.reference(), None);
        self.storage.put_entry(&entry).await?;
        self.emit(
            EntryAction::Update,
            Some(entry.clone()),
            Some(previous),
            actor,
        )
        .await;
        Ok(entry)
    }

    pub async fn delete_entry(&self, content_type: &str, id: Uuid, actor: &Actor) -> Result<()> {
        let ct = self.content_type(content_type).await?;
        let previous = self.require_entry(content_type, id).await?;
        actor.require_delete(&ct, &previous)?;
        self.run_before_hooks(
            EntryAction::Delete,
            &ct,
            previous.draft.clone(),
            actor,
            &Components::new(),
        )
        .await?;
        if !self.storage.delete_entry(content_type, id).await? {
            return Err(CmsError::NotFound(format!("{content_type} `{id}`")));
        }
        self.emit(EntryAction::Delete, None, Some(previous), actor)
            .await;
        Ok(())
    }

    /// Copies the current draft to the published stage. On workflow types
    /// only Admins may do this directly; others go through review.
    pub async fn publish_entry(
        &self,
        content_type: &str,
        id: Uuid,
        actor: &Actor,
    ) -> Result<Entry> {
        let ct = self.content_type(content_type).await?;
        actor.require_publish(&ct)?;
        self.publish(
            &ct,
            id,
            actor,
            EntryAction::Publish,
            HistoryAction::Published,
            None,
        )
        .await
    }

    async fn publish(
        &self,
        ct: &ContentType,
        id: Uuid,
        actor: &Actor,
        action: EntryAction,
        history: HistoryAction,
        comment: Option<String>,
    ) -> Result<Entry> {
        let mut entry = self.require_entry(&ct.api_id, id).await?;
        let previous = entry.clone();
        let components = self.components().await?;
        let data = normalize_entry_data(ct, current_fields(ct, &entry.draft), &components)?;
        let data = self
            .run_before_hooks(action, ct, data, actor, &components)
            .await?;
        self.check_references(ct, &data, &components).await?;
        entry.draft = data.clone();
        entry.published = Some(data);
        entry.published_at = Some(now());
        entry.status = WorkflowStatus::Published;
        entry.record(history, actor.reference(), comment);
        self.storage.put_entry(&entry).await?;
        self.emit(action, Some(entry.clone()), Some(previous), actor)
            .await;
        Ok(entry)
    }

    pub async fn unpublish_entry(
        &self,
        content_type: &str,
        id: Uuid,
        actor: &Actor,
    ) -> Result<Entry> {
        let ct = self.content_type(content_type).await?;
        actor.require_publish(&ct).or_else(|e| {
            if ct.workflow {
                actor.require_review(&ct)
            } else {
                Err(e)
            }
        })?;
        let mut entry = self.require_entry(content_type, id).await?;
        let previous = entry.clone();
        self.run_before_hooks(
            EntryAction::Unpublish,
            &ct,
            entry.draft.clone(),
            actor,
            &Components::new(),
        )
        .await?;
        entry.published = None;
        entry.published_at = None;
        if entry.status == WorkflowStatus::Published {
            entry.status = WorkflowStatus::Draft;
        }
        entry.record(HistoryAction::Unpublished, actor.reference(), None);
        self.storage.put_entry(&entry).await?;
        self.emit(
            EntryAction::Unpublish,
            Some(entry.clone()),
            Some(previous),
            actor,
        )
        .await;
        Ok(entry)
    }

    // ---- review workflow -----------------------------------------------

    async fn workflow_entry(&self, content_type: &str, id: Uuid) -> Result<(ContentType, Entry)> {
        let ct = self.content_type(content_type).await?;
        if !ct.workflow {
            return Err(CmsError::validation(format!(
                "the review workflow is not enabled on `{content_type}`"
            )));
        }
        let entry = self.require_entry(content_type, id).await?;
        Ok((ct, entry))
    }

    async fn set_status(
        &self,
        mut entry: Entry,
        ct: &ContentType,
        status: WorkflowStatus,
        (action, history): (EntryAction, HistoryAction),
        comment: Option<String>,
        actor: &Actor,
    ) -> Result<Entry> {
        let previous = entry.clone();
        self.run_before_hooks(action, ct, entry.draft.clone(), actor, &Components::new())
            .await?;
        entry.status = status;
        entry.record(history, actor.reference(), comment);
        self.storage.put_entry(&entry).await?;
        self.emit(action, Some(entry.clone()), Some(previous), actor)
            .await;
        Ok(entry)
    }

    /// `DRAFT | CHANGES_REQUESTED → IN_REVIEW`.
    pub async fn submit_for_review(
        &self,
        content_type: &str,
        id: Uuid,
        actor: &Actor,
    ) -> Result<Entry> {
        let (ct, entry) = self.workflow_entry(content_type, id).await?;
        actor.require_edit(&ct)?;
        if !matches!(
            entry.status,
            WorkflowStatus::Draft | WorkflowStatus::ChangesRequested
        ) {
            return Err(CmsError::Conflict(format!(
                "only drafts can be submitted (this entry is {})",
                entry.status.as_str()
            )));
        }
        // Reviewers must not receive something that cannot be published.
        normalize_entry_data(
            &ct,
            current_fields(&ct, &entry.draft),
            &self.components().await?,
        )?;
        let step = (EntryAction::Submit, HistoryAction::Submitted);
        self.set_status(entry, &ct, WorkflowStatus::InReview, step, None, actor)
            .await
    }

    /// `IN_REVIEW → PUBLISHED`.
    pub async fn approve(
        &self,
        content_type: &str,
        id: Uuid,
        comment: Option<String>,
        actor: &Actor,
    ) -> Result<Entry> {
        let (ct, entry) = self.workflow_entry(content_type, id).await?;
        actor.require_review(&ct)?;
        if entry.status != WorkflowStatus::InReview {
            return Err(CmsError::Conflict(format!(
                "the entry is not in review (it is {})",
                entry.status.as_str()
            )));
        }
        let comment = clean_comment(comment);
        self.publish(
            &ct,
            id,
            actor,
            EntryAction::Approve,
            HistoryAction::Approved,
            comment,
        )
        .await
    }

    /// `IN_REVIEW → CHANGES_REQUESTED`, with a mandatory comment.
    pub async fn request_changes(
        &self,
        content_type: &str,
        id: Uuid,
        comment: String,
        actor: &Actor,
    ) -> Result<Entry> {
        let (ct, entry) = self.workflow_entry(content_type, id).await?;
        actor.require_review(&ct)?;
        if entry.status != WorkflowStatus::InReview {
            return Err(CmsError::Conflict(format!(
                "the entry is not in review (it is {})",
                entry.status.as_str()
            )));
        }
        let comment = clean_comment(Some(comment))
            .ok_or_else(|| CmsError::validation("explain what needs to change in a comment"))?;
        let step = (EntryAction::RequestChanges, HistoryAction::ChangesRequested);
        self.set_status(
            entry,
            &ct,
            WorkflowStatus::ChangesRequested,
            step,
            Some(comment),
            actor,
        )
        .await
    }

    async fn require_entry(&self, content_type: &str, id: Uuid) -> Result<Entry> {
        self.storage
            .get_entry(content_type, id)
            .await?
            .ok_or_else(|| CmsError::NotFound(format!("{content_type} `{id}`")))
    }

    /// Checks a candidate against a PASSWORD field of an entry (in `stage`).
    pub async fn verify_password(
        &self,
        content_type: &str,
        id: Uuid,
        field: &str,
        stage: Stage,
        password: &str,
    ) -> Result<bool> {
        let ct = self.content_type(content_type).await?;
        if !ct
            .field(field)
            .is_some_and(|f| matches!(f.field_type, FieldType::Password { .. }))
        {
            return Err(CmsError::validation(format!(
                "`{content_type}.{field}` is not a password field"
            )));
        }
        let entry = self
            .get_entry(content_type, id, stage)
            .await?
            .ok_or_else(|| CmsError::NotFound(format!("{content_type} `{id}`")))?;
        let Some(hash) = entry
            .data(stage)
            .and_then(|d| d.get(field))
            .and_then(Value::as_str)
        else {
            return Ok(false);
        };
        crate::password::verify(password.to_string(), hash.to_string()).await
    }

    async fn check_references(
        &self,
        ct: &ContentType,
        data: &Map<String, Value>,
        components: &Components,
    ) -> Result<()> {
        for (target, id) in relation_refs(ct, data, components) {
            if self.storage.get_entry(&target, id).await?.is_none() {
                return Err(CmsError::validation(format!(
                    "referenced {target} `{id}` does not exist"
                )));
            }
        }
        Ok(())
    }
}

/// Hashes the PASSWORD values of client input (after checking their length).
/// Stored values are never passed here, so a client can't inject a hash.
async fn prepare_input(
    ct: &ContentType,
    mut data: Map<String, Value>,
) -> Result<Map<String, Value>> {
    for f in &ct.fields {
        let FieldType::Password { min_length } = f.field_type else {
            continue;
        };
        let Some(value) = data.get(&f.name).filter(|v| !v.is_null()) else {
            continue;
        };
        let plain = value
            .as_str()
            .ok_or_else(|| CmsError::validation(format!("field `{}` expects a string", f.name)))?;
        let len = plain.chars().count();
        if len < min_length as usize {
            return Err(CmsError::validation(format!(
                "field `{}` needs at least {min_length} characters",
                f.name
            )));
        }
        if plain.len() > 1024 {
            return Err(CmsError::validation(format!(
                "field `{}` is too long",
                f.name
            )));
        }
        let hashed = crate::password::hash(plain.to_string()).await?;
        data.insert(f.name.clone(), Value::String(hashed));
    }
    Ok(data)
}

/// Trims comments and bounds their size; empty means none.
fn clean_comment(comment: Option<String>) -> Option<String> {
    let c: String = comment?.trim().chars().take(2000).collect();
    (!c.is_empty()).then_some(c)
}

/// Drops values of fields that no longer exist in the content type.
fn current_fields(ct: &ContentType, data: &Map<String, Value>) -> Map<String, Value> {
    data.iter()
        .filter(|(k, _)| ct.field(k).is_some())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::access::Role;
    use crate::memory::MemoryStorage;
    use crate::model::FieldDef;

    fn user(name: &str, role: Role, scope: &[&str]) -> Actor {
        Actor {
            id: Some(Uuid::new_v4()),
            name: name.into(),
            role,
            scope: scope.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn data(title: &str) -> Map<String, Value> {
        json!({ "title": title }).as_object().unwrap().clone()
    }

    async fn setup() -> Cms {
        let cms = Cms::new(Arc::new(MemoryStorage::new()));
        for (api_id, workflow) in [("Page", true), ("Note", false)] {
            let def = ContentTypeDef {
                api_id: api_id.into(),
                plural_api_id: None,
                description: None,
                fields: vec![FieldDef {
                    name: "title".into(),
                    field_type: FieldType::text(),
                    required: true,
                    description: None,
                }],
                workflow,
            };
            cms.create_content_type(def, &Actor::system())
                .await
                .unwrap();
        }
        cms
    }

    fn forbidden<T: std::fmt::Debug>(r: Result<T>) {
        assert!(
            matches!(r, Err(CmsError::Forbidden(_))),
            "expected Forbidden, got {r:?}"
        );
    }

    #[tokio::test]
    async fn roles_without_workflow() {
        let cms = setup().await;
        let contributor = user("C", Role::Contributor, &[]);
        let scoped = user("S", Role::Contributor, &["Page"]);
        let reviewer = user("R", Role::Reviewer, &[]);
        let member = user("M", Role::Member, &[]);

        let note = cms
            .create_entry("Note", data("a"), &contributor)
            .await
            .unwrap();
        assert_eq!(note.created_by.as_ref().unwrap().name, "C");
        let published = cms
            .publish_entry("Note", note.id, &contributor)
            .await
            .unwrap();
        assert_eq!(published.status, WorkflowStatus::Published);

        forbidden(cms.create_entry("Note", data("x"), &scoped).await); // out of scope
        forbidden(cms.create_entry("Note", data("x"), &reviewer).await); // reviewers do not write
        forbidden(cms.create_entry("Note", data("x"), &member).await);
        forbidden(cms.publish_entry("Note", note.id, &reviewer).await);
        forbidden(cms.delete_entry("Note", note.id, &contributor).await); // published: admins only
        forbidden(
            cms.create_content_type(
                ContentTypeDef {
                    workflow: false,
                    api_id: "X".into(),
                    plural_api_id: None,
                    description: None,
                    fields: vec![],
                },
                &contributor,
            )
            .await,
        );

        let draft = cms
            .create_entry("Note", data("b"), &contributor)
            .await
            .unwrap();
        cms.delete_entry("Note", draft.id, &contributor)
            .await
            .unwrap(); // never published: ok
        cms.delete_entry("Note", note.id, &user("A", Role::Admin, &[]))
            .await
            .unwrap();

        let r = cms
            .submit_for_review("Note", published.id, &contributor)
            .await;
        assert!(
            matches!(r, Err(CmsError::Validation(_)) | Err(CmsError::NotFound(_))),
            "{r:?}"
        );
    }

    #[tokio::test]
    async fn review_workflow() {
        let cms = setup().await;
        let contributor = user("C", Role::Contributor, &["Page"]);
        let reviewer = user("R", Role::Reviewer, &["Page"]);
        let other_reviewer = user("O", Role::Reviewer, &["Note"]);
        let admin = user("A", Role::Admin, &[]);

        let page = cms
            .create_entry("Page", data("v1"), &contributor)
            .await
            .unwrap();
        forbidden(cms.publish_entry("Page", page.id, &contributor).await);
        let r = cms.approve("Page", page.id, None, &reviewer).await;
        assert!(
            matches!(r, Err(CmsError::Conflict(_))),
            "not in review yet: {r:?}"
        );

        let e = cms
            .submit_for_review("Page", page.id, &contributor)
            .await
            .unwrap();
        assert_eq!(e.status, WorkflowStatus::InReview);
        forbidden(cms.approve("Page", page.id, None, &other_reviewer).await); // out of scope
        forbidden(cms.approve("Page", page.id, None, &contributor).await);
        let r = cms
            .request_changes("Page", page.id, "  ".into(), &reviewer)
            .await;
        assert!(
            matches!(r, Err(CmsError::Validation(_))),
            "comment required: {r:?}"
        );
        let e = cms
            .request_changes("Page", page.id, "Fix the title".into(), &reviewer)
            .await
            .unwrap();
        assert_eq!(e.status, WorkflowStatus::ChangesRequested);
        assert_eq!(
            e.history.last().unwrap().comment.as_deref(),
            Some("Fix the title")
        );

        cms.update_entry("Page", page.id, data("v2"), &contributor)
            .await
            .unwrap();
        cms.submit_for_review("Page", page.id, &contributor)
            .await
            .unwrap();
        // Editing during review sends it back to draft.
        let e = cms
            .update_entry("Page", page.id, data("v3"), &contributor)
            .await
            .unwrap();
        assert_eq!(e.status, WorkflowStatus::Draft);
        cms.submit_for_review("Page", page.id, &contributor)
            .await
            .unwrap();
        let r = cms.submit_for_review("Page", page.id, &contributor).await;
        assert!(matches!(r, Err(CmsError::Conflict(_))), "{r:?}");

        let e = cms
            .approve("Page", page.id, Some("Looks good".into()), &reviewer)
            .await
            .unwrap();
        assert_eq!(e.status, WorkflowStatus::Published);
        assert_eq!(e.published.as_ref().unwrap()["title"], "v3");
        let actions: Vec<_> = e.history.iter().map(|h| h.action).collect();
        use HistoryAction::*;
        assert_eq!(
            actions,
            vec![
                Created,
                Submitted,
                ChangesRequested,
                Updated,
                Submitted,
                Updated,
                Submitted,
                Approved
            ]
        );

        // Admins can bypass review; reviewers may unpublish workflow types.
        let p2 = cms
            .create_entry("Page", data("direct"), &admin)
            .await
            .unwrap();
        cms.publish_entry("Page", p2.id, &admin).await.unwrap();
        cms.unpublish_entry("Page", p2.id, &reviewer).await.unwrap();
        forbidden(cms.unpublish_entry("Page", page.id, &contributor).await);
    }
}

#[cfg(test)]
mod field_type_tests {
    use serde_json::json;

    use super::*;
    use crate::memory::MemoryStorage;
    use crate::model::FieldDef;

    fn f(name: &str, field_type: FieldType) -> FieldDef {
        FieldDef {
            name: name.into(),
            field_type,
            required: false,
            description: None,
        }
    }

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn invalid<T: std::fmt::Debug>(r: Result<T>, needle: &str) {
        match r {
            Err(CmsError::Validation(m)) | Err(CmsError::Conflict(m)) => {
                assert!(m.contains(needle), "{m}")
            }
            other => panic!("expected an error containing {needle:?}, got {other:?}"),
        }
    }

    async fn cms_with(fields: Vec<FieldDef>) -> Cms {
        let cms = Cms::new(Arc::new(MemoryStorage::new()));
        let sys = Actor::system();
        cms.create_component(
            ComponentDef {
                api_id: "Link".into(),
                description: None,
                fields: vec![
                    FieldDef {
                        required: true,
                        ..f(
                            "label",
                            FieldType::Text {
                                min_length: None,
                                max_length: Some(20),
                                pattern: None,
                            },
                        )
                    },
                    f(
                        "url",
                        FieldType::Text {
                            min_length: None,
                            max_length: None,
                            pattern: Some(r"https://\S+".into()),
                        },
                    ),
                ],
            },
            &sys,
        )
        .await
        .unwrap();
        cms.create_component(
            ComponentDef {
                api_id: "Hero".into(),
                description: None,
                fields: vec![
                    f("title", FieldType::text()),
                    f(
                        "cta",
                        FieldType::Component {
                            component: "Link".into(),
                            repeatable: false,
                        },
                    ),
                ],
            },
            &sys,
        )
        .await
        .unwrap();
        cms.create_content_type(
            ContentTypeDef {
                api_id: "Page".into(),
                plural_api_id: None,
                description: None,
                fields,
                workflow: false,
            },
            &sys,
        )
        .await
        .unwrap();
        cms
    }

    #[tokio::test]
    async fn text_markdown_date_uuid() {
        let cms = cms_with(vec![
            f(
                "email",
                FieldType::Text {
                    min_length: Some(6),
                    max_length: Some(80),
                    pattern: Some(r"[^@\s]+@[^@\s]+\.[a-z]{2,}".into()),
                },
            ),
            f("body", FieldType::Markdown),
            f("day", FieldType::Date),
            f("code", FieldType::Uuid),
        ])
        .await;
        let sys = Actor::system();
        let e = cms
            .create_entry("Page", obj(json!({"email": "ada@example.com", "body": "# Hi <Callout>ok</Callout>", "day": "2026-10-18"})), &sys)
            .await
            .unwrap();
        assert_eq!(e.draft["day"], "2026-10-18");
        assert!(
            Uuid::parse_str(e.draft["code"].as_str().unwrap()).is_ok(),
            "uuid generated"
        );
        invalid(
            cms.create_entry("Page", obj(json!({"email": "not an email"})), &sys)
                .await,
            "format",
        );
        invalid(
            cms.create_entry("Page", obj(json!({"email": "a@b.c"})), &sys)
                .await,
            "at least 6",
        );
        invalid(
            cms.create_entry("Page", obj(json!({"body": "<div>html</div>"})), &sys)
                .await,
            "contains HTML",
        );
        invalid(
            cms.create_entry("Page", obj(json!({"day": "2026-02-30"})), &sys)
                .await,
            "a date",
        );
        invalid(
            cms.create_entry("Page", obj(json!({"code": "nope"})), &sys)
                .await,
            "a UUID",
        );
        let given = "6F9619FF-8B86-D011-B42D-00CF4FC964FF";
        let e = cms
            .create_entry("Page", obj(json!({"code": given})), &sys)
            .await
            .unwrap();
        assert_eq!(e.draft["code"], given.to_lowercase());
    }

    #[tokio::test]
    async fn passwords_are_hashed_and_verifiable() {
        let cms = cms_with(vec![
            f("title", FieldType::text()),
            f("secret", FieldType::Password { min_length: 8 }),
        ])
        .await;
        let sys = Actor::system();
        invalid(
            cms.create_entry("Page", obj(json!({"secret": "short"})), &sys)
                .await,
            "at least 8",
        );
        let e = cms
            .create_entry(
                "Page",
                obj(json!({"title": "x", "secret": "correct horse"})),
                &sys,
            )
            .await
            .unwrap();
        let stored = e.draft["secret"].as_str().unwrap();
        assert!(stored.starts_with("$argon2id$") && !stored.contains("correct horse"));
        assert!(
            cms.verify_password("Page", e.id, "secret", Stage::Draft, "correct horse")
                .await
                .unwrap()
        );
        assert!(
            !cms.verify_password("Page", e.id, "secret", Stage::Draft, "wrong")
                .await
                .unwrap()
        );
        // Updating other fields keeps the hash; sending a "hash" re-hashes it.
        let e2 = cms
            .update_entry("Page", e.id, obj(json!({"title": "y"})), &sys)
            .await
            .unwrap();
        assert_eq!(e2.draft["secret"], e.draft["secret"]);
        let e3 = cms
            .update_entry("Page", e.id, obj(json!({"secret": stored})), &sys)
            .await
            .unwrap();
        assert_ne!(e3.draft["secret"], e.draft["secret"]);
        assert!(
            cms.verify_password("Page", e.id, "secret", Stage::Draft, stored)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn components_and_dynamic_zones() {
        let cms = cms_with(vec![
            f("title", FieldType::text()),
            f(
                "links",
                FieldType::Component {
                    component: "Link".into(),
                    repeatable: true,
                },
            ),
            f(
                "blocks",
                FieldType::DynamicZone {
                    components: vec!["Hero".into(), "Link".into()],
                },
            ),
        ])
        .await;
        let sys = Actor::system();
        let e = cms
            .create_entry(
                "Page",
                obj(json!({
                    "links": [{"label": "Docs", "url": "https://tessheadless.com/docs/"}],
                    "blocks": [
                        {"__component": "Hero", "title": "Welcome", "cta": {"label": "Start", "url": "https://example.com"}},
                        {"__component": "Link", "label": "More"}
                    ]
                })),
                &sys,
            )
            .await
            .unwrap();
        assert_eq!(e.draft["blocks"][0]["cta"]["label"], "Start");
        assert_eq!(e.draft["blocks"][1]["__component"], "Link");
        // Nested validation, with paths in messages.
        invalid(
            cms.create_entry(
                "Page",
                obj(json!({"links": [{"url": "https://x.y"}]})),
                &sys,
            )
            .await,
            "links[0].label",
        );
        invalid(
            cms.create_entry(
                "Page",
                obj(json!({"links": [{"label": "x", "colour": 1}]})),
                &sys,
            )
            .await,
            "unknown field `links[0].colour`",
        );
        invalid(
            cms.create_entry("Page", obj(json!({"blocks": [{"title": "no type"}]})), &sys)
                .await,
            "__component",
        );
        invalid(
            cms.create_entry(
                "Page",
                obj(json!({"blocks": [{"__component": "Nope"}]})),
                &sys,
            )
            .await,
            "not allowed",
        );
        invalid(
            cms.create_entry("Page", obj(json!({"blocks": [{"__component": "Hero", "cta": {"label": "x", "url": "ftp://x"}}]})), &sys).await,
            "blocks[0].cta.url",
        );

        // Definitions: cycles, dynamic zones and passwords inside components, deletion in use.
        let link_with_hero = ComponentDef {
            api_id: "Link".into(),
            description: None,
            fields: vec![
                f("label", FieldType::text()),
                f(
                    "hero",
                    FieldType::Component {
                        component: "Hero".into(),
                        repeatable: false,
                    },
                ),
            ],
        };
        invalid(
            cms.update_component("Link", link_with_hero, &sys).await,
            "cannot contain themselves",
        );
        let dz = ComponentDef {
            api_id: "Bad".into(),
            description: None,
            fields: vec![f(
                "z",
                FieldType::DynamicZone {
                    components: vec!["Link".into()],
                },
            )],
        };
        invalid(
            cms.create_component(dz, &sys).await,
            "dynamic zones are not allowed",
        );
        let pw = ComponentDef {
            api_id: "Bad".into(),
            description: None,
            fields: vec![f("p", FieldType::Password { min_length: 8 })],
        };
        invalid(
            cms.create_component(pw, &sys).await,
            "passwords are only allowed",
        );
        invalid(
            cms.delete_component("Link", &sys).await,
            "still used by Page, Hero",
        );
        let clash = ComponentDef {
            api_id: "Page".into(),
            description: None,
            fields: vec![f("x", FieldType::text())],
        };
        invalid(
            cms.create_component(clash, &sys).await,
            "already the name of a content type",
        );
        let contributor = Actor {
            id: None,
            name: "C".into(),
            role: crate::Role::Contributor,
            scope: vec![],
        };
        let tag = ComponentDef {
            api_id: "Tag".into(),
            description: None,
            fields: vec![f("x", FieldType::text())],
        };
        assert!(matches!(
            cms.create_component(tag, &contributor).await,
            Err(CmsError::Forbidden(_))
        ));
    }

    #[test]
    fn legacy_string_and_text_load_as_unbounded_text() {
        for legacy in [
            r#"{"name":"a","type":"string"}"#,
            r#"{"name":"a","type":"text","required":true}"#,
        ] {
            let f: FieldDef = serde_json::from_str(legacy).unwrap();
            assert_eq!(f.field_type, FieldType::text());
        }
    }
}
