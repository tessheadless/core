// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! GraphQL API layer of the CMS.
//!
//! The schema is generated from the content types stored in the database and
//! rebuilt when they change: immediately on the instance that made the change,
//! and within `refresh_interval` on other instances (other containers, other
//! Lambda execution environments).

mod accounts;
mod admin;
mod components;
mod fields;
pub mod filter;
mod i18n;
pub mod ids;
mod jsonobj;
mod media;
mod plugins;
mod search;
mod webhooks;
mod workflow;
pub mod naming;
mod schema;

use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub use async_graphql;
use async_graphql::dynamic::Schema;
use tess_core::{Actor, Cms, CmsError, Stage, Storage};
use tokio::sync::RwLock;

use crate::ids::IdCodec;
use crate::schema::SchemaDirty;

/// Who is making a request; resolved by the HTTP layer from credentials.
#[derive(Debug, Clone, PartialEq)]
pub enum Principal {
    /// No credentials and public reads disabled: no access.
    Anonymous,
    /// No credentials: read-only access to published content.
    Public,
    /// A signed-in user, API token or the bootstrap token. What it may do is
    /// decided by its role (see `tess_core::access`).
    User(Actor),
}

impl Principal {
    pub fn can_read(&self, stage: Stage) -> bool {
        match self {
            Principal::User(_) => true,
            Principal::Public => stage == Stage::Published,
            Principal::Anonymous => false,
        }
    }

    pub fn actor(&self) -> Option<&Actor> {
        match self {
            Principal::User(a) => Some(a),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SchemaConfig {
    pub default_page_size: usize,
    pub max_page_size: usize,
    pub max_depth: usize,
    pub introspection: bool,
    /// How long a schema is trusted before checking the database for
    /// content model changes made by other instances.
    pub refresh_interval: Duration,
}

impl Default for SchemaConfig {
    fn default() -> Self {
        SchemaConfig {
            default_page_size: 100,
            max_page_size: 1000,
            max_depth: 16,
            introspection: true,
            refresh_interval: Duration::from_secs(10),
        }
    }
}

/// Optional subsystems exposed through the API.
#[derive(Clone, Default)]
pub struct Services {
    pub media: Option<tess_media::MediaService>,
    pub plugins: Option<Arc<tess_plugins::PluginManager>>,
    pub search: Option<tess_search::SearchService>,
    pub auth: Option<Arc<tess_auth::AuthService>>,
    pub i18n: Option<Arc<tess_i18n::I18nService>>,
    pub webhooks: Option<Arc<tess_webhooks::WebhookService>>,
    /// Embedded workflow editor (n8n), when configured.
    pub workflows: Option<WorkflowsInfo>,
}

/// Where the admin UI finds the workflow editor.
#[derive(Debug, Clone)]
pub struct WorkflowsInfo {
    /// `n8n`.
    pub provider: String,
    /// Same-origin path of the editor, e.g. `/n8n/`.
    pub editor_url: String,
}

struct Built {
    schema: Schema,
    fingerprint: u64,
    checked_at: Instant,
}

pub struct GraphqlService {
    cms: Cms,
    codec: Arc<IdCodec>,
    config: SchemaConfig,
    services: Services,
    dirty: Arc<AtomicBool>,
    built: RwLock<Option<Built>>,
}

impl GraphqlService {
    pub fn new(storage: Arc<dyn Storage>, codec: IdCodec, config: SchemaConfig) -> Self {
        Self::with_services(storage, codec, config, Services::default())
    }

    pub fn with_services(storage: Arc<dyn Storage>, codec: IdCodec, config: SchemaConfig, services: Services) -> Self {
        let cms = Cms::new(storage).with_schema_guard(Arc::new(naming::check_conflicts));
        GraphqlService {
            cms,
            codec: Arc::new(codec),
            config,
            services,
            dirty: Arc::new(AtomicBool::new(false)),
            built: RwLock::new(None),
        }
    }

    pub fn cms(&self) -> &Cms {
        &self.cms
    }

    pub fn codec(&self) -> &IdCodec {
        &self.codec
    }

    pub fn services(&self) -> &Services {
        &self.services
    }

    /// Rebuilds the schema on the next request (e.g. another instance changed
    /// the content model).
    pub fn mark_schema_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Installs built-in content types and hooks of the enabled services.
    pub async fn install(&self) -> Result<(), CmsError> {
        if let Some(media) = &self.services.media {
            media.install(&self.cms).await?;
        }
        if let Some(plugins) = &self.services.plugins {
            plugins.refresh(&self.cms).await?;
        }
        if let Some(search) = &self.services.search {
            search.install(&self.cms);
        }
        if let Some(webhooks) = &self.services.webhooks {
            self.cms.hooks().add(Arc::new(tess_webhooks::WebhookHook::new(webhooks.clone())));
            webhooks.register_events(tess_i18n::I18N_EVENTS);
        }
        if let Some(i18n) = &self.services.i18n {
            i18n.install(&self.cms);
        }
        self.dirty.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// The current schema, rebuilt if the content model changed.
    pub async fn schema(&self) -> Result<Schema, CmsError> {
        if let Some(b) = self.built.read().await.as_ref()
            && !self.dirty.load(Ordering::SeqCst)
            && b.checked_at.elapsed() < self.config.refresh_interval
        {
            return Ok(b.schema.clone());
        }

        let mut guard = self.built.write().await;
        let was_dirty = self.dirty.swap(false, Ordering::SeqCst);
        if let Some(b) = guard.as_ref()
            && !was_dirty
            && b.checked_at.elapsed() < self.config.refresh_interval
        {
            return Ok(b.schema.clone()); // refreshed by a concurrent request
        }
        if let Some(plugins) = &self.services.plugins
            && let Err(e) = plugins.refresh(&self.cms).await
        {
            tracing::error!(error = %e, "plugin refresh failed");
        }
        let content_types = self.cms.content_types().await?;
        let components = self.cms.components().await?;
        // Content types with search enabled get a `search<Plural>` query.
        let searchable: BTreeSet<String> = match &self.services.search {
            Some(search) => {
                let settings = search.all_settings().await?;
                content_types
                    .iter()
                    .filter(|ct| settings.get(&ct.api_id).and_then(|s| tess_search::SearchIndex::new(ct, s)).is_some())
                    .map(|ct| ct.api_id.clone())
                    .collect()
            }
            None => BTreeSet::new(),
        };
        let fingerprint = {
            let mut h = DefaultHasher::new();
            serde_json::to_string(&content_types).unwrap_or_default().hash(&mut h);
            serde_json::to_string(&components).unwrap_or_default().hash(&mut h);
            searchable.hash(&mut h);
            h.finish()
        };
        if let Some(b) = guard.as_mut()
            && b.fingerprint == fingerprint
        {
            b.checked_at = Instant::now();
            return Ok(b.schema.clone());
        }
        let model = schema::Model { content_types: &content_types, components: &components, searchable: &searchable };
        let schema = schema::build(
            model,
            &self.config,
            self.cms.clone(),
            self.codec.clone(),
            SchemaDirty(self.dirty.clone()),
            &self.services,
        )
        .map_err(|e| CmsError::Storage(format!("cannot build GraphQL schema: {e}")))?;
        tracing::info!(content_types = content_types.len(), "GraphQL schema rebuilt");
        *guard = Some(Built { schema: schema.clone(), fingerprint, checked_at: Instant::now() });
        Ok(schema)
    }

    /// Executes a request as `principal`.
    pub async fn execute(&self, request: async_graphql::Request, principal: Principal) -> async_graphql::Response {
        match self.schema().await {
            Ok(schema) => schema.execute(request.data(principal)).await,
            Err(e) => {
                tracing::error!(error = %e, "schema unavailable");
                async_graphql::Response::from_errors(vec![async_graphql::ServerError::new(
                    "service temporarily unavailable",
                    None,
                )])
            }
        }
    }

    /// SDL of the current schema.
    pub async fn sdl(&self) -> Result<String, CmsError> {
        Ok(self.schema().await?.sdl())
    }
}
