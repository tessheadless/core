// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::sync::Arc;
use std::time::Duration;

use tess_core::memory::MemoryStorage;
use tess_core::{Actor, CmsError, Role};

use super::*;

const PW: &str = "correct horse battery";

fn service() -> AuthService {
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).unwrap();
    AuthService::new(Arc::new(MemoryStorage::new()), SessionKey::new(&secret).unwrap(), Duration::from_secs(3600))
}

fn new_user(email: &str, role: Role) -> NewUser {
    NewUser { email: email.into(), name: format!("{role:?} Example"), role, scope: vec![], password: PW.into() }
}

#[tokio::test]
async fn login_sessions_and_revocation() {
    let auth = service();
    assert!(!auth.has_users().await.unwrap());
    let owner = auth.create_user(&Actor::system(), new_user("Owner@Example.com ", Role::Owner)).await.unwrap();
    assert_eq!(owner.email, "owner@example.com");
    assert!(auth.has_users().await.unwrap());

    assert!(matches!(auth.login("owner@example.com", "wrong password!!").await, Err(CmsError::Forbidden(_))));
    assert!(matches!(auth.login("nobody@example.com", PW).await, Err(CmsError::Forbidden(_))));
    let session = auth.login("OWNER@example.com", PW).await.unwrap();
    let actor = auth.authenticate(&session.token).await.unwrap().unwrap();
    assert_eq!((actor.id, actor.role), (Some(owner.id), Role::Owner));

    // Tampering with the token is detected.
    let mut forged = session.token.clone();
    forged.insert(10, 'x');
    assert!(auth.authenticate(&forged).await.is_err());
    assert!(auth.authenticate("unrelated").await.unwrap().is_none());

    // Signing out everywhere revokes the session.
    auth.sign_out_everywhere(&actor).await.unwrap();
    assert!(auth.authenticate(&session.token).await.is_err());

    // Changing the password returns a working new session.
    let s2 = auth.login("owner@example.com", PW).await.unwrap();
    let a2 = auth.authenticate(&s2.token).await.unwrap().unwrap();
    assert!(auth.change_password(&a2, "not my password", "another long password").await.is_err());
    let s3 = auth.change_password(&a2, PW, "another long password").await.unwrap();
    assert!(auth.authenticate(&s2.token).await.is_err());
    assert!(auth.authenticate(&s3.token).await.unwrap().is_some());
}

#[tokio::test]
async fn expired_sessions_are_rejected() {
    let mut secret = [7u8; 32];
    getrandom::fill(&mut secret).unwrap();
    let key = SessionKey::new(&secret).unwrap();
    let auth = AuthService::new(Arc::new(MemoryStorage::new()), SessionKey::new(&secret).unwrap(), Duration::from_secs(60));
    let u = auth.create_user(&Actor::system(), new_user("a@example.com", Role::Member)).await.unwrap();
    let expired = key.sign(u.id, 0, chrono::Utc::now() - chrono::Duration::seconds(5));
    assert!(matches!(auth.authenticate(&expired).await, Err(CmsError::Forbidden(m)) if m.contains("expired")));
    assert!(SessionKey::new(b"too short").is_none());
}

#[tokio::test]
async fn lockout_after_failures() {
    let auth = service();
    auth.create_user(&Actor::system(), new_user("m@example.com", Role::Member)).await.unwrap();
    for _ in 0..MAX_FAILED_LOGINS {
        let _ = auth.login("m@example.com", "bad password 123").await;
    }
    let r = auth.login("m@example.com", PW).await;
    assert!(matches!(&r, Err(CmsError::Forbidden(m)) if m.contains("too many")), "{r:?}");
}

#[tokio::test]
async fn management_rules() {
    let auth = service();
    let sys = Actor::system();
    let owner = auth.create_user(&sys, new_user("o@example.com", Role::Owner)).await.unwrap();
    let admin_user = auth.create_user(&sys, new_user("a@example.com", Role::Admin)).await.unwrap();
    let admin = Actor { id: Some(admin_user.id), name: "A".into(), role: Role::Admin, scope: vec![] };
    let owner_actor = Actor { id: Some(owner.id), name: "O".into(), role: Role::Owner, scope: vec![] };
    let contributor = Actor { id: None, name: "C".into(), role: Role::Contributor, scope: vec![] };

    // Validation and uniqueness.
    let mut weak = new_user("w@example.com", Role::Member);
    weak.password = "short".into();
    assert!(matches!(auth.create_user(&admin, weak).await, Err(CmsError::Validation(_))));
    assert!(matches!(auth.create_user(&admin, new_user("not-an-email", Role::Member)).await, Err(CmsError::Validation(_))));
    assert!(matches!(auth.create_user(&admin, new_user("A@example.com", Role::Member)).await, Err(CmsError::Conflict(_))));

    // Only Owners manage Owners; non-admins manage nobody.
    assert!(matches!(auth.create_user(&admin, new_user("o2@example.com", Role::Owner)).await, Err(CmsError::Forbidden(_))));
    assert!(matches!(auth.create_user(&contributor, new_user("x@example.com", Role::Member)).await, Err(CmsError::Forbidden(_))));
    let up = UserUpdate { name: Some("Renamed".into()), ..Default::default() };
    assert!(matches!(auth.update_user(&admin, owner.id, up).await, Err(CmsError::Forbidden(_))));
    assert!(matches!(auth.list_users(&contributor).await, Err(CmsError::Forbidden(_))));

    // The last Owner cannot be demoted, disabled or deleted; nobody edits their own role.
    let demote = UserUpdate { role: Some(Role::Admin), ..Default::default() };
    assert!(auth.update_user(&sys, owner.id, demote.clone()).await.is_err());
    assert!(auth.delete_user(&sys, owner.id).await.is_err());
    let promote = UserUpdate { role: Some(Role::Owner), ..Default::default() };
    assert!(matches!(auth.update_user(&admin, admin_user.id, promote.clone()).await, Err(CmsError::Forbidden(_))));
    auth.update_user(&owner_actor, admin_user.id, promote).await.unwrap();
    auth.update_user(&owner_actor, admin_user.id, demote).await.unwrap();

    // Role changes revoke sessions.
    let m = auth.create_user(&admin, new_user("m@example.com", Role::Member)).await.unwrap();
    let s = auth.login("m@example.com", PW).await.unwrap();
    auth.update_user(&admin, m.id, UserUpdate { role: Some(Role::Contributor), scope: Some(vec!["Post".into()]), ..Default::default() }).await.unwrap();
    assert!(auth.authenticate(&s.token).await.is_err());
    let a = auth.authenticate(&auth.login("m@example.com", PW).await.unwrap().token).await.unwrap().unwrap();
    assert_eq!((a.role, a.scope.clone()), (Role::Contributor, vec!["Post".to_string()]));

    // Disabled users cannot sign in; deleting frees the email.
    auth.update_user(&admin, m.id, UserUpdate { disabled: Some(true), ..Default::default() }).await.unwrap();
    assert!(auth.login("m@example.com", PW).await.is_err());
    auth.delete_user(&admin, m.id).await.unwrap();
    auth.create_user(&admin, new_user("m@example.com", Role::Member)).await.unwrap();
    assert_eq!(auth.list_users(&admin).await.unwrap().len(), 3);
}

#[tokio::test]
async fn api_tokens() {
    let auth = service();
    let admin = Actor { id: None, name: "A".into(), role: Role::Admin, scope: vec![] };
    assert!(auth.create_api_token(&admin, "nope", Role::Owner, vec![], None).await.is_err());
    let (t, secret) = auth.create_api_token(&admin, "Website build", Role::Member, vec![], None).await.unwrap();
    let actor = auth.authenticate(&secret).await.unwrap().unwrap();
    assert_eq!(actor.role, Role::Member);
    assert!(actor.name.contains("Website build"));
    // Wrong secret, same id.
    let forged = format!("{}{}", &secret[..secret.len() - 2], "AA");
    assert!(auth.authenticate(&forged).await.is_err());
    assert_eq!(auth.list_api_tokens(&admin).await.unwrap().len(), 1);
    auth.revoke_api_token(&admin, &t.id).await.unwrap();
    assert!(auth.authenticate(&secret).await.is_err());

    let (_, short) = auth.create_api_token(&admin, "Short", Role::Contributor, vec![], Some(Duration::from_secs(0))).await.unwrap();
    std::thread::sleep(Duration::from_millis(10));
    assert!(matches!(auth.authenticate(&short).await, Err(CmsError::Forbidden(m)) if m.contains("expired")));
}
