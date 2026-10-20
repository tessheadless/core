// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Users, sessions and API tokens.
//!
//! - **Users** sign in with email + password (Argon2id). Accounts lock for 15
//!   minutes after repeated failures.
//! - **Sessions** are stateless signed tokens (`tess_s1.<claims>.<hmac>`),
//!   usable on AWS Lambda. Every request reloads the user, and each user has a
//!   `session_version`: bumping it (role change, disable, password change,
//!   "sign out everywhere") revokes all their sessions at once.
//! - **API tokens** (`tess_t1_<id>_<secret>`) carry a role and a scope, for
//!   CI, front-ends and webhooks. Only a SHA-256 of the secret is stored.
//!
//! Everything is kept in the [`Storage`] settings store, so it works on every
//! database backend.

mod password;
mod token;

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tess_core::model::now;
use tess_core::{Actor, CmsError, Result, Role, Storage};
use uuid::Uuid;

pub use token::SessionKey;

const USERS: &str = "users/";
const EMAILS: &str = "users-by-email/";
const TOKENS: &str = "api-tokens/";
const MIN_PASSWORD: usize = 12;
const MAX_FAILED_LOGINS: u32 = 10;
const LOCKOUT_MINUTES: i64 = 15;

/// Stored user (settings key `users/{id}`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserRecord {
    id: Uuid,
    email: String,
    name: String,
    role: Role,
    #[serde(default)]
    scope: Vec<String>,
    password_hash: String,
    #[serde(default)]
    disabled: bool,
    #[serde(default)]
    session_version: u32,
    #[serde(default)]
    failed_logins: u32,
    #[serde(default)]
    locked_until: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(default)]
    last_login_at: Option<DateTime<Utc>>,
}

/// A user as exposed by the API (no secrets).
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: Role,
    pub scope: Vec<String>,
    pub disabled: bool,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

impl From<&UserRecord> for User {
    fn from(r: &UserRecord) -> Self {
        User {
            id: r.id,
            email: r.email.clone(),
            name: r.name.clone(),
            role: r.role,
            scope: r.scope.clone(),
            disabled: r.disabled,
            created_at: r.created_at,
            last_login_at: r.last_login_at,
        }
    }
}

impl UserRecord {
    fn actor(&self) -> Actor {
        Actor {
            id: Some(self.id),
            name: self.name.clone(),
            role: self.role,
            scope: self.scope.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub email: String,
    pub name: String,
    pub role: Role,
    pub scope: Vec<String>,
    pub password: String,
}

/// Changes to a user; `None` keeps the current value.
#[derive(Debug, Clone, Default)]
pub struct UserUpdate {
    pub name: Option<String>,
    pub role: Option<Role>,
    pub scope: Option<Vec<String>>,
    pub disabled: Option<bool>,
    pub password: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub user: User,
}

/// Stored API token (settings key `api-tokens/{id}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiToken {
    pub id: String,
    pub name: String,
    pub role: Role,
    #[serde(default)]
    pub scope: Vec<String>,
    /// SHA-256 of the secret, hex.
    secret_hash: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

pub struct AuthService {
    storage: Arc<dyn Storage>,
    key: SessionKey,
    session_ttl: Duration,
}

fn invalid_login() -> CmsError {
    CmsError::Forbidden("invalid email or password".into())
}

fn invalid_credentials() -> CmsError {
    CmsError::Forbidden("invalid or revoked credentials".into())
}

fn normalize_email(email: &str) -> Result<String> {
    let email = email.trim().to_lowercase();
    let valid = email.len() <= 254
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        })
        && !email.chars().any(|c| c.is_whitespace() || c == '/');
    if valid {
        Ok(email)
    } else {
        Err(CmsError::validation("invalid email address"))
    }
}

fn check_password(password: &str) -> Result<()> {
    if password.chars().count() < MIN_PASSWORD {
        return Err(CmsError::validation(format!(
            "passwords need at least {MIN_PASSWORD} characters"
        )));
    }
    if password.len() > 1024 {
        return Err(CmsError::validation("password too long"));
    }
    Ok(())
}

fn check_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(CmsError::validation("name must be 1 to 100 characters"));
    }
    Ok(name.to_string())
}

impl AuthService {
    pub fn new(storage: Arc<dyn Storage>, key: SessionKey, session_ttl: Duration) -> Self {
        AuthService {
            storage,
            key,
            session_ttl,
        }
    }

    // ---- storage helpers ---------------------------------------------

    async fn records(&self) -> Result<Vec<UserRecord>> {
        self.storage
            .list_settings(USERS)
            .await?
            .into_iter()
            .map(|(_, v)| serde_json::from_value(v).map_err(CmsError::storage))
            .collect()
    }

    async fn record(&self, id: Uuid) -> Result<UserRecord> {
        let v = self
            .storage
            .get_setting(&format!("{USERS}{id}"))
            .await?
            .ok_or_else(|| CmsError::NotFound("user".into()))?;
        serde_json::from_value(v).map_err(CmsError::storage)
    }

    async fn record_by_email(&self, email: &str) -> Result<Option<UserRecord>> {
        let Some(idx) = self
            .storage
            .get_setting(&format!("{EMAILS}{email}"))
            .await?
        else {
            return Ok(None);
        };
        let Some(id) = idx
            .get("id")
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
        else {
            return Ok(None);
        };
        match self.record(id).await {
            Ok(r) => Ok(Some(r)),
            Err(CmsError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn save(&self, r: &UserRecord) -> Result<()> {
        let v = serde_json::to_value(r).map_err(CmsError::storage)?;
        self.storage
            .put_setting(&format!("{USERS}{}", r.id), &v)
            .await
    }

    async fn active_owners(&self) -> Result<usize> {
        Ok(self
            .records()
            .await?
            .iter()
            .filter(|r| r.role == Role::Owner && !r.disabled)
            .count())
    }

    /// Whether any account exists (before that, only the bootstrap token works).
    pub async fn has_users(&self) -> Result<bool> {
        Ok(!self.storage.list_settings(USERS).await?.is_empty())
    }

    // ---- user management ---------------------------------------------

    pub async fn list_users(&self, actor: &Actor) -> Result<Vec<User>> {
        actor.require_admin()?;
        let mut users: Vec<User> = self.records().await?.iter().map(User::from).collect();
        users.sort_by_key(|u| u.name.to_lowercase());
        Ok(users)
    }

    pub async fn get_user(&self, id: Uuid) -> Result<User> {
        Ok(User::from(&self.record(id).await?))
    }

    /// Admins manage users; only Owners manage Owners.
    fn check_can_manage(actor: &Actor, role: Role) -> Result<()> {
        actor.require_admin()?;
        if role == Role::Owner && actor.role != Role::Owner {
            return Err(CmsError::Forbidden(
                "only an Owner can manage Owners".into(),
            ));
        }
        Ok(())
    }

    pub async fn create_user(&self, actor: &Actor, new: NewUser) -> Result<User> {
        Self::check_can_manage(actor, new.role)?;
        let email = normalize_email(&new.email)?;
        let name = check_name(&new.name)?;
        check_password(&new.password)?;
        if self.record_by_email(&email).await?.is_some() {
            return Err(CmsError::Conflict(
                "a user with this email already exists".into(),
            ));
        }
        let ts = now();
        let record = UserRecord {
            id: Uuid::new_v4(),
            email: email.clone(),
            name,
            role: new.role,
            scope: new.scope,
            password_hash: password::hash(new.password).await?,
            disabled: false,
            session_version: 0,
            failed_logins: 0,
            locked_until: None,
            created_at: ts,
            updated_at: ts,
            last_login_at: None,
        };
        self.save(&record).await?;
        self.storage
            .put_setting(&format!("{EMAILS}{email}"), &json!({ "id": record.id }))
            .await?;
        Ok(User::from(&record))
    }

    pub async fn update_user(&self, actor: &Actor, id: Uuid, update: UserUpdate) -> Result<User> {
        let mut r = self.record(id).await?;
        Self::check_can_manage(actor, r.role)?;
        let is_self = actor.id == Some(id);
        let mut revoke = false;
        if let Some(name) = update.name {
            r.name = check_name(&name)?;
        }
        if let Some(role) = update.role
            && role != r.role
        {
            Self::check_can_manage(actor, role)?;
            if is_self {
                return Err(CmsError::Forbidden(
                    "you cannot change your own role".into(),
                ));
            }
            if r.role == Role::Owner && !r.disabled && self.active_owners().await? <= 1 {
                return Err(CmsError::Conflict(
                    "there must always be at least one Owner".into(),
                ));
            }
            r.role = role;
            revoke = true;
        }
        if let Some(scope) = update.scope
            && scope != r.scope
        {
            r.scope = scope;
            revoke = true;
        }
        if let Some(disabled) = update.disabled
            && disabled != r.disabled
        {
            if is_self {
                return Err(CmsError::Forbidden(
                    "you cannot disable your own account".into(),
                ));
            }
            if disabled && r.role == Role::Owner && self.active_owners().await? <= 1 {
                return Err(CmsError::Conflict(
                    "there must always be at least one Owner".into(),
                ));
            }
            r.disabled = disabled;
            revoke = true;
        }
        if let Some(password) = update.password {
            check_password(&password)?;
            r.password_hash = password::hash(password).await?;
            r.failed_logins = 0;
            r.locked_until = None;
            revoke = true;
        }
        if revoke {
            r.session_version += 1;
        }
        r.updated_at = now();
        self.save(&r).await?;
        Ok(User::from(&r))
    }

    pub async fn delete_user(&self, actor: &Actor, id: Uuid) -> Result<()> {
        let r = self.record(id).await?;
        Self::check_can_manage(actor, r.role)?;
        if actor.id == Some(id) {
            return Err(CmsError::Forbidden(
                "you cannot delete your own account".into(),
            ));
        }
        if r.role == Role::Owner && !r.disabled && self.active_owners().await? <= 1 {
            return Err(CmsError::Conflict(
                "there must always be at least one Owner".into(),
            ));
        }
        self.storage.delete_setting(&format!("{USERS}{id}")).await?;
        self.storage
            .delete_setting(&format!("{EMAILS}{}", r.email))
            .await?;
        Ok(())
    }

    // ---- sessions ------------------------------------------------------

    fn issue(&self, r: &UserRecord) -> Session {
        let expires_at = Utc::now() + self.session_ttl;
        Session {
            token: self.key.sign(r.id, r.session_version, expires_at),
            expires_at,
            user: User::from(r),
        }
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<Session> {
        let found = match normalize_email(email) {
            Ok(email) => self.record_by_email(&email).await?,
            Err(_) => None,
        };
        let Some(mut r) = found else {
            // Same cost as a real check, so timing does not reveal accounts.
            password::burn(password.to_string()).await;
            return Err(invalid_login());
        };
        if r.locked_until.is_some_and(|t| t > Utc::now()) {
            return Err(CmsError::Forbidden(
                "too many failed attempts, try again later".into(),
            ));
        }
        if !password::verify(password.to_string(), r.password_hash.clone()).await? {
            r.failed_logins += 1;
            if r.failed_logins >= MAX_FAILED_LOGINS {
                r.failed_logins = 0;
                r.locked_until = Some(Utc::now() + chrono::Duration::minutes(LOCKOUT_MINUTES));
                tracing::warn!(user = %r.id, "account locked after repeated failed logins");
            }
            self.save(&r).await?;
            return Err(invalid_login());
        }
        if r.disabled {
            return Err(invalid_login());
        }
        r.failed_logins = 0;
        r.locked_until = None;
        r.last_login_at = Some(now());
        self.save(&r).await?;
        Ok(self.issue(&r))
    }

    /// Changes the caller's password; all other sessions are revoked and a
    /// fresh session is returned.
    pub async fn change_password(
        &self,
        actor: &Actor,
        current: &str,
        new: &str,
    ) -> Result<Session> {
        let id = actor
            .id
            .ok_or_else(|| CmsError::Forbidden("not a user session".into()))?;
        let mut r = self.record(id).await?;
        if !password::verify(current.to_string(), r.password_hash.clone()).await? {
            return Err(CmsError::Forbidden("current password is wrong".into()));
        }
        check_password(new)?;
        r.password_hash = password::hash(new.to_string()).await?;
        r.session_version += 1;
        r.updated_at = now();
        self.save(&r).await?;
        Ok(self.issue(&r))
    }

    /// Revokes every session of the caller (including the current one).
    pub async fn sign_out_everywhere(&self, actor: &Actor) -> Result<()> {
        let id = actor
            .id
            .ok_or_else(|| CmsError::Forbidden("not a user session".into()))?;
        let mut r = self.record(id).await?;
        r.session_version += 1;
        self.save(&r).await
    }

    /// Resolves a bearer credential to an actor. `Ok(None)` when it is neither
    /// a session nor an API token (the caller may try other schemes); an error
    /// when it is one but invalid, expired or revoked.
    pub async fn authenticate(&self, bearer: &str) -> Result<Option<Actor>> {
        if token::is_session(bearer) {
            let (id, version, expires_at) =
                self.key.verify(bearer).ok_or_else(invalid_credentials)?;
            if expires_at < Utc::now() {
                return Err(CmsError::Forbidden("session expired".into()));
            }
            let r = self.record(id).await.map_err(|_| invalid_credentials())?;
            if r.disabled || r.session_version != version {
                return Err(invalid_credentials());
            }
            return Ok(Some(r.actor()));
        }
        if let Some((id, secret)) = token::parse_api_token(bearer) {
            let t = self.api_token(id).await?.ok_or_else(invalid_credentials)?;
            if !token::secret_matches(secret, &t.secret_hash) {
                return Err(invalid_credentials());
            }
            if t.expires_at.is_some_and(|e| e < Utc::now()) {
                return Err(CmsError::Forbidden("API token expired".into()));
            }
            return Ok(Some(Actor {
                id: None,
                name: format!("API token \"{}\"", t.name),
                role: t.role,
                scope: t.scope,
            }));
        }
        Ok(None)
    }

    // ---- API tokens ----------------------------------------------------

    async fn api_token(&self, id: &str) -> Result<Option<ApiToken>> {
        self.storage
            .get_setting(&format!("{TOKENS}{id}"))
            .await?
            .map(|v| serde_json::from_value(v).map_err(CmsError::storage))
            .transpose()
    }

    pub async fn list_api_tokens(&self, actor: &Actor) -> Result<Vec<ApiToken>> {
        actor.require_admin()?;
        let mut tokens: Vec<ApiToken> = self
            .storage
            .list_settings(TOKENS)
            .await?
            .into_iter()
            .map(|(_, v)| serde_json::from_value(v).map_err(CmsError::storage))
            .collect::<Result<_>>()?;
        tokens.sort_by_key(|t| std::cmp::Reverse(t.created_at));
        Ok(tokens)
    }

    /// Returns the token record and the secret token string (shown once).
    pub async fn create_api_token(
        &self,
        actor: &Actor,
        name: &str,
        role: Role,
        scope: Vec<String>,
        expires_in: Option<Duration>,
    ) -> Result<(ApiToken, String)> {
        Self::check_can_manage(actor, role)?;
        let name = check_name(name)?;
        let (id, secret, secret_hash) = token::new_api_token();
        let t = ApiToken {
            id: id.clone(),
            name,
            role,
            scope,
            secret_hash,
            created_by: actor.name.clone(),
            created_at: now(),
            expires_at: expires_in.map(|d| Utc::now() + d),
        };
        let v = serde_json::to_value(&t).map_err(CmsError::storage)?;
        self.storage
            .put_setting(&format!("{TOKENS}{id}"), &v)
            .await?;
        Ok((t, secret))
    }

    pub async fn revoke_api_token(&self, actor: &Actor, id: &str) -> Result<()> {
        actor.require_admin()?;
        if !self
            .storage
            .delete_setting(&format!("{TOKENS}{id}"))
            .await?
        {
            return Err(CmsError::NotFound("API token".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
