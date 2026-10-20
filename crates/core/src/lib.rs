// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Core domain of the CMS: content models, entries, the query/filter language,
//! the [`Storage`] abstraction implemented by every database backend, and the
//! [`Cms`] service that holds the business rules shared by all backends.

pub mod access;
pub mod error;
pub mod hooks;
pub mod memory;
pub mod model;
pub mod password;
pub mod query;
pub mod redact;
pub mod secrets;
pub mod service;
pub mod storage;
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
pub mod validate;

pub use access::{Actor, ActorRef, Role};
pub use error::{CmsError, Result};
pub use hooks::{EntryAction, EntryEvent, Hook, HookRegistry, MODEL_EVENTS, SystemEvent};
pub use model::{
    Component, ComponentDef, ContentType, ContentTypeDef, Entry, FieldDef, FieldType,
    HistoryAction, HistoryEvent, Stage, WorkflowStatus,
};
pub use query::{Direction, EntryQuery, Filter, Op, Page, SortKey};
pub use service::Cms;
pub use storage::Storage;
