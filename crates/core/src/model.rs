// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::access::ActorRef;

/// System fields exposed on every entry. They are reserved and cannot be used
/// as content field names, which lets backends store them inside the
/// per-stage documents and strip them back out when loading.
pub const SYSTEM_FIELDS: &[&str] =
    &["id", "createdAt", "updatedAt", "publishedAt", "workflowStatus", "createdBy", "updatedBy", "history"];

/// Number of history events kept per entry.
pub const HISTORY_LIMIT: usize = 50;

/// Default `maxLength` of TEXT fields created through the API.
pub const DEFAULT_TEXT_MAX_LENGTH: u32 = 80;
/// Default `minLength` of PASSWORD fields.
pub const DEFAULT_PASSWORD_MIN_LENGTH: u32 = 8;

/// The data type of a content field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FieldType {
    /// Plain text, with optional length bounds and a format (regex the whole
    /// value must match). Models stored before TEXT replaced STRING/TEXT load
    /// as TEXT without bounds.
    #[serde(alias = "string")]
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_length: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_length: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
    },
    /// Markdown or MDX source, rendered by clients. Raw HTML is rejected.
    Markdown,
    Int,
    Float,
    Boolean,
    /// RFC 3339 timestamp, normalized to UTC with millisecond precision.
    DateTime,
    /// Calendar date without time, `YYYY-MM-DD`.
    Date,
    /// Arbitrary JSON value.
    Json,
    /// UUID, normalized to lowercase; generated when missing on create.
    Uuid,
    /// Write-only secret, stored as an Argon2id hash and never returned.
    Password {
        #[serde(default = "default_password_min")]
        min_length: u32,
    },
    Enum { values: Vec<String> },
    /// Reference to entries of another (or the same) content type.
    Relation {
        target: String,
        #[serde(default)]
        many: bool,
    },
    /// A reusable group of fields (see [`Component`]), stored inline.
    Component {
        component: String,
        #[serde(default)]
        repeatable: bool,
    },
    /// An ordered list of components, each one of the allowed `components`
    /// (stored with a `__component` discriminator). Not allowed in components.
    DynamicZone { components: Vec<String> },
}

fn default_password_min() -> u32 {
    DEFAULT_PASSWORD_MIN_LENGTH
}

impl FieldType {
    /// TEXT without bounds or format.
    pub fn text() -> Self {
        FieldType::Text { min_length: None, max_length: None, pattern: None }
    }

    /// Whether values of this type can be compared with `<`, `>`, ... and sorted.
    pub fn is_ordered(&self) -> bool {
        matches!(
            self,
            FieldType::Text { .. }
                | FieldType::Markdown
                | FieldType::Int
                | FieldType::Float
                | FieldType::DateTime
                | FieldType::Date
                | FieldType::Enum { .. }
        )
    }

    /// Searchable, `_contains`-filterable text.
    pub fn is_textual(&self) -> bool {
        matches!(self, FieldType::Text { .. } | FieldType::Markdown)
    }

    /// Never returned by the API, never indexed.
    pub fn is_secret(&self) -> bool {
        matches!(self, FieldType::Password { .. })
    }
}

/// A reusable set of fields, used by COMPONENT and DYNAMIC_ZONE fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// PascalCase identifier, e.g. `Seo`. Shares the GraphQL type namespace
    /// with content types.
    pub api_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub fields: Vec<FieldDef>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Component {
    pub fn field(&self, name: &str) -> Option<&FieldDef> {
        self.fields.iter().find(|f| f.name == name)
    }
}

/// User-supplied definition of a component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentDef {
    pub api_id: String,
    #[serde(default)]
    pub description: Option<String>,
    pub fields: Vec<FieldDef>,
}

/// Key under which dynamic-zone items store their component's apiId.
pub const COMPONENT_KEY: &str = "__component";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDef {
    pub name: String,
    #[serde(flatten)]
    pub field_type: FieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// User-supplied definition of a content type, as received from the API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentTypeDef {
    /// PascalCase identifier, e.g. `BlogPost`. Becomes the GraphQL type name.
    pub api_id: String,
    /// camelCase plural used for list queries, e.g. `blogPosts`.
    /// Defaults to the singular form plus `s`.
    #[serde(default)]
    pub plural_api_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub fields: Vec<FieldDef>,
    /// Require review before publishing (see [`WorkflowStatus`]).
    #[serde(default)]
    pub workflow: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentType {
    pub api_id: String,
    pub plural_api_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub fields: Vec<FieldDef>,
    #[serde(default)]
    pub workflow: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ContentType {
    /// camelCase singular, e.g. `blogPost` for `BlogPost`.
    pub fn singular_api_id(&self) -> String {
        lower_first(&self.api_id)
    }

    pub fn field(&self, name: &str) -> Option<&FieldDef> {
        self.fields.iter().find(|f| f.name == name)
    }
}

pub fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

pub fn upper_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Stage {
    Draft,
    Published,
}

/// State of an entry's draft.
///
/// ```text
/// DRAFT ──submit──▶ IN_REVIEW ──approve──▶ PUBLISHED
///   ▲                   │
///   └──── CHANGES_REQUESTED ◀── request changes
/// ```
/// Editing any entry sets it back to `DRAFT`. Without workflow, publishing
/// goes straight from `DRAFT` to `PUBLISHED`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowStatus {
    #[default]
    Draft,
    InReview,
    ChangesRequested,
    /// The draft is identical to the published version.
    Published,
}

impl WorkflowStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            WorkflowStatus::Draft => "DRAFT",
            WorkflowStatus::InReview => "IN_REVIEW",
            WorkflowStatus::ChangesRequested => "CHANGES_REQUESTED",
            WorkflowStatus::Published => "PUBLISHED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HistoryAction {
    Created,
    Updated,
    Submitted,
    ChangesRequested,
    Approved,
    Published,
    Unpublished,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEvent {
    pub action: HistoryAction,
    pub at: DateTime<Utc>,
    pub by: ActorRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// A content entry. Editing always happens on `draft`; publishing copies the
/// draft into `published`, which is what public clients read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: Uuid,
    pub content_type: String,
    pub draft: Map<String, Value>,
    pub published: Option<Map<String, Value>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub status: WorkflowStatus,
    #[serde(default)]
    pub created_by: Option<ActorRef>,
    #[serde(default)]
    pub updated_by: Option<ActorRef>,
    /// Most recent last, at most [`HISTORY_LIMIT`] events.
    #[serde(default)]
    pub history: Vec<HistoryEvent>,
}

impl Entry {
    pub fn data(&self, stage: Stage) -> Option<&Map<String, Value>> {
        match stage {
            Stage::Draft => Some(&self.draft),
            Stage::Published => self.published.as_ref(),
        }
    }

    /// The document clients see for a stage: content fields plus system
    /// fields. Filters and sorting are always evaluated against this shape.
    pub fn view(&self, stage: Stage) -> Option<Map<String, Value>> {
        let mut doc = self.data(stage)?.clone();
        doc.insert("id".into(), Value::String(self.id.to_string()));
        doc.insert("createdAt".into(), Value::String(format_ts(self.created_at)));
        doc.insert("updatedAt".into(), Value::String(format_ts(self.updated_at)));
        if let Some(ts) = self.published_at {
            doc.insert("publishedAt".into(), Value::String(format_ts(ts)));
        }
        doc.insert("workflowStatus".into(), Value::String(self.status.as_str().into()));
        if let Some(by) = &self.created_by {
            doc.insert("createdBy".into(), serde_json::to_value(by).unwrap_or_default());
        }
        if let Some(by) = &self.updated_by {
            doc.insert("updatedBy".into(), serde_json::to_value(by).unwrap_or_default());
        }
        if !self.history.is_empty() {
            doc.insert("history".into(), serde_json::to_value(&self.history).unwrap_or_default());
        }
        Some(doc)
    }

    /// Rebuilds an entry from stored views (backends that persist
    /// [`Entry::view`] documents, like SQL and MongoDB).
    pub fn from_views(
        content_type: String,
        id: Uuid,
        draft_view: Map<String, Value>,
        published_view: Option<Map<String, Value>>,
    ) -> Result<Entry, String> {
        let ts = |k: &str| draft_view.get(k).and_then(Value::as_str).and_then(parse_ts);
        fn parse<T: serde::de::DeserializeOwned>(doc: &Map<String, Value>, k: &str) -> Result<Option<T>, String> {
            doc.get(k).cloned().map(serde_json::from_value).transpose().map_err(|e| format!("`{k}`: {e}"))
        }
        Ok(Entry {
            id,
            content_type,
            created_at: ts("createdAt").ok_or("entry document lacks `createdAt`")?,
            updated_at: ts("updatedAt").ok_or("entry document lacks `updatedAt`")?,
            published_at: ts("publishedAt"),
            status: parse(&draft_view, "workflowStatus")?.unwrap_or_default(),
            created_by: parse(&draft_view, "createdBy")?,
            updated_by: parse(&draft_view, "updatedBy")?,
            history: parse(&draft_view, "history")?.unwrap_or_default(),
            published: published_view.map(strip_system_fields),
            draft: strip_system_fields(draft_view),
        })
    }

    pub(crate) fn record(&mut self, action: HistoryAction, by: ActorRef, comment: Option<String>) {
        self.history.push(HistoryEvent { action, at: now(), by, comment });
        if self.history.len() > HISTORY_LIMIT {
            let excess = self.history.len() - HISTORY_LIMIT;
            self.history.drain(..excess);
        }
    }
}


/// Removes system fields from a stored view, giving back the content data.
pub fn strip_system_fields(mut doc: Map<String, Value>) -> Map<String, Value> {
    for f in SYSTEM_FIELDS {
        doc.remove(*f);
    }
    doc
}

/// Canonical timestamp format. Fixed width and always UTC, so lexicographic
/// order matches chronological order in every backend.
pub fn format_ts(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

/// Current time truncated to the precision of [`format_ts`], so values
/// survive a round trip through any backend unchanged.
pub fn now() -> DateTime<Utc> {
    parse_ts(&format_ts(Utc::now())).expect("canonical timestamp parses")
}
