// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Forms: Admins define custom forms (contact, reservation, sign-up…), your
//! front-ends render them from their public definition and post submissions,
//! Tess validates and stores them, and emits a `form.submitted` event that
//! webhooks (and n8n's Tess Trigger) receive.
//!
//! Definitions and submissions live in the settings store of every backend.

mod service;
mod validate;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

pub use service::{FormInput, FormsConfig, FormsService, RateLimiter, SubmitError, SubmitMeta};
pub use validate::{check_definition, validate_submission};

/// Events emitted by forms (for webhooks).
pub const FORM_EVENTS: &[&str] = &[
    "form.submitted",
    "form.create",
    "form.update",
    "form.delete",
];

/// Name of the hidden anti-spam field: bots fill it, people don't see it.
pub const HONEYPOT: &str = "_hp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FieldKind {
    Text,
    Textarea,
    Email,
    Number,
    /// `YYYY-MM-DD`.
    Date,
    /// `YYYY-MM-DDTHH:MM` (a local date and time, e.g. a reservation slot).
    Datetime,
    /// One of `options`.
    Select,
    /// Several of `options`.
    MultiSelect,
    Checkbox,
    Url,
    Phone,
}

impl FieldKind {
    pub const ALL: [FieldKind; 11] = [
        FieldKind::Text,
        FieldKind::Textarea,
        FieldKind::Email,
        FieldKind::Number,
        FieldKind::Date,
        FieldKind::Datetime,
        FieldKind::Select,
        FieldKind::MultiSelect,
        FieldKind::Checkbox,
        FieldKind::Url,
        FieldKind::Phone,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            FieldKind::Text => "TEXT",
            FieldKind::Textarea => "TEXTAREA",
            FieldKind::Email => "EMAIL",
            FieldKind::Number => "NUMBER",
            FieldKind::Date => "DATE",
            FieldKind::Datetime => "DATETIME",
            FieldKind::Select => "SELECT",
            FieldKind::MultiSelect => "MULTI_SELECT",
            FieldKind::Checkbox => "CHECKBOX",
            FieldKind::Url => "URL",
            FieldKind::Phone => "PHONE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormField {
    /// Key of the value in submissions, e.g. `email`.
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: FieldKind,
    #[serde(default)]
    pub required: bool,
    /// Help text shown under the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// `SELECT` and `MULTI_SELECT` choices.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// Text fields: maximum length (defaults: 1,000; 10,000 for `TEXTAREA`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    /// `NUMBER` bounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

/// A form, as Admins define it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormDef {
    pub id: Uuid,
    /// Public name, in the URL: `/api/forms/<slug>`.
    pub slug: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub fields: Vec<FormField>,
    /// Disabled forms refuse submissions (and their definition is not public).
    pub enabled: bool,
    /// Shown by front-ends after a successful submission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_message: Option<String>,
    /// Where plain HTML forms (`application/x-www-form-urlencoded`) are sent
    /// back after a submission (a 303 redirect); JSON submissions get JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: String,
}

impl FormDef {
    /// What visitors' front-ends may read (`GET /api/forms/<slug>`).
    pub fn public_json(&self) -> Value {
        serde_json::json!({
            "slug": self.slug,
            "title": self.title,
            "description": self.description,
            "fields": self.fields,
            "successMessage": self.success_message,
            "honeypot": HONEYPOT,
        })
    }
}

/// A stored submission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Submission {
    pub id: Uuid,
    pub form_id: Uuid,
    /// The validated values, by field name.
    pub data: Map<String, Value>,
    pub submitted_at: String,
    /// Page the form was posted from (`Referer`), when sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}
