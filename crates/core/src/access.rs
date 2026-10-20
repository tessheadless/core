// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Who may do what. Every write goes through [`crate::Cms`] with an
//! [`Actor`], and the rules below are enforced there, so all API surfaces
//! (GraphQL, REST uploads, plugins) behave the same.
//!
//! | capability                                         | Owner | Admin | Contributor | Reviewer | Member |
//! |----------------------------------------------------|:-----:|:-----:|:-----------:|:--------:|:------:|
//! | manage Owners                                      |   ✔   |       |             |          |        |
//! | manage users, API tokens, models, plugins          |   ✔   |   ✔   |             |          |        |
//! | publish anything, bypass review                    |   ✔   |   ✔   |             |          |        |
//! | approve / request changes (workflow types)         |   ✔   |   ✔   |      –      |    ✔     |        |
//! | create & edit drafts, submit, upload media         |   ✔   |   ✔   |      ✔      |          |        |
//! | publish types *without* workflow                   |   ✔   |   ✔   |      ✔      |          |        |
//! | read drafts and media                              |   ✔   |   ✔   |      ✔      |    ✔     |   ✔    |
//!
//! Contributors and Reviewers are limited to their **scope** (a list of
//! content types; empty means all).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{CmsError, Result};
use crate::model::{ContentType, Entry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Role {
    Member,
    Reviewer,
    Contributor,
    Admin,
    Owner,
}

impl Role {
    pub const ALL: [Role; 5] = [Role::Owner, Role::Admin, Role::Contributor, Role::Reviewer, Role::Member];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "OWNER",
            Role::Admin => "ADMIN",
            Role::Contributor => "CONTRIBUTOR",
            Role::Reviewer => "REVIEWER",
            Role::Member => "MEMBER",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.as_str() == s)
    }

    pub fn is_admin(self) -> bool {
        matches!(self, Role::Owner | Role::Admin)
    }
}

/// Reference to whoever made a change, as stored on entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActorRef {
    /// `None` for the system (bootstrap token, internal jobs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    pub name: String,
}

/// The authenticated caller of an operation.
#[derive(Debug, Clone, PartialEq)]
pub struct Actor {
    pub id: Option<Uuid>,
    pub name: String,
    pub role: Role,
    /// Content types a Contributor/Reviewer works on; empty means all.
    pub scope: Vec<String>,
}

fn forbidden(msg: impl Into<String>) -> CmsError {
    CmsError::Forbidden(msg.into())
}

impl Actor {
    /// Full access, for the bootstrap admin token and internal jobs.
    pub fn system() -> Self {
        Actor { id: None, name: "system".into(), role: Role::Owner, scope: vec![] }
    }

    pub fn reference(&self) -> ActorRef {
        ActorRef { id: self.id, name: self.name.clone() }
    }

    pub fn in_scope(&self, content_type: &str) -> bool {
        self.role.is_admin() || self.scope.is_empty() || self.scope.iter().any(|s| s == content_type)
    }

    pub fn require_admin(&self) -> Result<()> {
        if self.role.is_admin() { Ok(()) } else { Err(forbidden("requires the Admin role")) }
    }

    /// Create and edit drafts, submit for review.
    pub fn require_edit(&self, ct: &ContentType) -> Result<()> {
        let role_ok = matches!(self.role, Role::Owner | Role::Admin | Role::Contributor);
        if role_ok && self.in_scope(&ct.api_id) {
            Ok(())
        } else {
            Err(forbidden(format!("you cannot edit `{}` entries", ct.api_id)))
        }
    }

    /// Publish or unpublish directly (without review).
    pub fn require_publish(&self, ct: &ContentType) -> Result<()> {
        if self.role.is_admin() {
            return Ok(());
        }
        if ct.workflow {
            return Err(forbidden(format!("`{}` requires review: submit it for review instead", ct.api_id)));
        }
        self.require_edit(ct)
    }

    /// Approve or request changes on a workflow type.
    pub fn require_review(&self, ct: &ContentType) -> Result<()> {
        let role_ok = matches!(self.role, Role::Owner | Role::Admin | Role::Reviewer);
        if role_ok && self.in_scope(&ct.api_id) {
            Ok(())
        } else {
            Err(forbidden(format!("you cannot review `{}` entries", ct.api_id)))
        }
    }

    /// Deleting published content is an editorial decision: Contributors may
    /// only delete entries that were never published.
    pub fn require_delete(&self, ct: &ContentType, entry: &Entry) -> Result<()> {
        if self.role.is_admin() {
            return Ok(());
        }
        self.require_edit(ct)?;
        if entry.published.is_some() {
            return Err(forbidden("published entries can only be deleted by an Admin"));
        }
        Ok(())
    }
}
