// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Users, roles and the review workflow, end to end through GraphQL.

use std::sync::Arc;
use std::time::Duration;

use tess_auth::{AuthService, SessionKey};
use tess_core::memory::MemoryStorage;
use tess_core::{Actor, Storage};
use tess_graphql::ids::{IdCodec, IdConfig};
use tess_graphql::{GraphqlService, Principal, SchemaConfig, Services};
use serde_json::{Value, json};

fn secret() -> Vec<u8> {
    let mut b = vec![0u8; 32];
    getrandom::fill(&mut b).unwrap();
    b
}

struct Env {
    svc: GraphqlService,
    auth: Arc<AuthService>,
}

impl Env {
    fn new() -> Self {
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let auth = Arc::new(AuthService::new(storage.clone(), SessionKey::new(&secret()).unwrap(), Duration::from_secs(3600)));
        let codec = IdCodec::new(IdConfig {
            secret: secret(),
            previous_secrets: vec![],
            ref_secret: secret(),
            ttl: Duration::from_secs(3600),
        })
        .unwrap();
        let services = Services { auth: Some(auth.clone()), ..Default::default() };
        Env { svc: GraphqlService::with_services(storage, codec, SchemaConfig::default(), services), auth }
    }

    /// Resolves a bearer token the way the HTTP layer does.
    async fn principal(&self, token: &str) -> Principal {
        Principal::User(self.auth.authenticate(token).await.unwrap().expect("valid token"))
    }

    async fn run(&self, who: &Principal, query: &str, vars: Value) -> Value {
        let req = async_graphql::Request::new(query).variables(async_graphql::Variables::from_json(vars));
        serde_json::to_value(self.svc.execute(req, who.clone()).await).unwrap()
    }

    async fn ok(&self, who: &Principal, query: &str, vars: Value) -> Value {
        let res = self.run(who, query, vars).await;
        assert!(res.get("errors").is_none(), "unexpected errors: {res:#}");
        res["data"].clone()
    }

    async fn code(&self, who: &Principal, query: &str, vars: Value) -> String {
        let res = self.run(who, query, vars).await;
        res["errors"][0]["extensions"]["code"].as_str().unwrap_or("<no error>").to_string()
    }

    async fn login(&self, email: &str) -> Principal {
        let token = self.login_token(email).await;
        self.principal(&token).await
    }

    async fn login_token(&self, email: &str) -> String {
        let d = self
            .ok(
                &Principal::Public,
                "mutation($e: String!, $p: String!) { login(email: $e, password: $p) { token user { role } } }",
                json!({"e": email, "p": PW}),
            )
            .await;
        d["login"]["token"].as_str().unwrap().to_string()
    }
}

const PW: &str = "a long enough password";

#[tokio::test]
async fn users_roles_and_review_workflow() {
    let env = Env::new();
    let public = Principal::Public;
    let system = Principal::User(Actor::system());

    // Bootstrap: the admin token creates the first Owner.
    assert_eq!(env.ok(&public, "{ setupRequired }", json!({})).await["setupRequired"], true);
    let create_user = "mutation($i: UserInput!) { createUser(input: $i) { id role scope } }";
    env.ok(&system, create_user, json!({"i": {"email": "owner@example.com", "name": "Olivia Owner", "role": "OWNER", "password": PW}}))
        .await;
    assert_eq!(env.ok(&public, "{ setupRequired }", json!({})).await["setupRequired"], false);
    assert_eq!(
        env.code(&public, "mutation { login(email: \"owner@example.com\", password: \"nope nope nope\") { token } }", json!({})).await,
        "FORBIDDEN"
    );
    let owner = env.login("owner@example.com").await;
    assert_eq!(env.ok(&owner, "{ me { name role } }", json!({})).await["me"], json!({"name": "Olivia Owner", "role": "OWNER"}));

    for (email, name, role, scope) in [
        ("c@example.com", "Carl Contributor", "CONTRIBUTOR", json!(["Page"])),
        ("r@example.com", "Rita Reviewer", "REVIEWER", json!(["Page"])),
        ("m@example.com", "Max Member", "MEMBER", json!([])),
    ] {
        env.ok(&owner, create_user, json!({"i": {"email": email, "name": name, "role": role, "scope": scope, "password": PW}}))
            .await;
    }
    let contributor = env.login("c@example.com").await;
    let reviewer = env.login("r@example.com").await;
    let member = env.login("m@example.com").await;
    assert_eq!(env.code(&contributor, "{ users { name } }", json!({})).await, "FORBIDDEN");
    assert_eq!(env.ok(&owner, "{ users { name } }", json!({})).await["users"].as_array().unwrap().len(), 4);

    // Content models: only admins.
    let model = r#"mutation { a: createContentType(input: {apiId: "Page", workflow: true, fields: [{name: "title", type: TEXT, required: true}]}) { workflow }
                              b: createContentType(input: {apiId: "Note", fields: [{name: "title", type: TEXT, required: true}]}) { workflow } }"#;
    assert_eq!(env.code(&contributor, model, json!({})).await, "FORBIDDEN");
    assert_eq!(env.code(&member, model, json!({})).await, "FORBIDDEN");
    let d = env.ok(&owner, model, json!({})).await;
    assert_eq!((d["a"]["workflow"].clone(), d["b"]["workflow"].clone()), (json!(true), json!(false)));

    // Every signed-in user can read the definitions (content manager forms), not anonymous clients.
    let defs = env.ok(&member, "{ contentTypes { apiId workflow } components { apiId } }", json!({})).await;
    assert_eq!(defs["contentTypes"].as_array().unwrap().len(), 2);
    assert_eq!(env.code(&public, "{ contentTypes { apiId } }", json!({})).await, "FORBIDDEN");

    // Contributor drafts a page; publishing directly is refused on workflow types.
    let d = env.ok(&contributor, r#"mutation { createPage(data: {title: "Hello"}) { id workflowStatus createdBy } }"#, json!({})).await;
    assert_eq!(d["createPage"]["workflowStatus"], "DRAFT");
    assert_eq!(d["createPage"]["createdBy"], "Carl Contributor");
    let id = d["createPage"]["id"].as_str().unwrap().to_string();
    let v = json!({"id": id});
    assert_eq!(env.code(&contributor, "mutation($id: ID!) { publishPage(id: $id) { id } }", v.clone()).await, "FORBIDDEN");
    // Out of scope and read-only roles.
    assert_eq!(env.code(&contributor, r#"mutation { createNote(data: {title: "x"}) { id } }"#, json!({})).await, "FORBIDDEN");
    assert_eq!(env.code(&member, r#"mutation { createPage(data: {title: "x"}) { id } }"#, json!({})).await, "FORBIDDEN");
    assert_eq!(env.code(&reviewer, r#"mutation { createPage(data: {title: "x"}) { id } }"#, json!({})).await, "FORBIDDEN");

    // Review cycle.
    let d = env.ok(&contributor, "mutation($id: ID!) { submitPageForReview(id: $id) { workflowStatus } }", v.clone()).await;
    assert_eq!(d["submitPageForReview"]["workflowStatus"], "IN_REVIEW");
    let queue = env.ok(&reviewer, "{ pages(where: {workflowStatus: IN_REVIEW}, stage: DRAFT) { total items { title } } }", json!({})).await;
    assert_eq!(queue["pages"]["total"], 1);
    assert_eq!(
        env.code(&reviewer, "mutation($id: ID!) { requestChangesPage(id: $id, comment: \"\") { id } }", v.clone()).await,
        "BAD_USER_INPUT"
    );
    let d = env
        .ok(&reviewer, "mutation($id: ID!) { requestChangesPage(id: $id, comment: \"Add a subtitle\") { workflowStatus } }", v.clone())
        .await;
    assert_eq!(d["requestChangesPage"]["workflowStatus"], "CHANGES_REQUESTED");
    env.ok(&contributor, r#"mutation($id: ID!) { updatePage(id: $id, data: {title: "Hello, world"}) { id } }"#, v.clone()).await;
    env.ok(&contributor, "mutation($id: ID!) { submitPageForReview(id: $id) { id } }", v.clone()).await;
    assert_eq!(env.code(&contributor, "mutation($id: ID!) { approvePage(id: $id) { id } }", v.clone()).await, "FORBIDDEN");
    let d = env.ok(&reviewer, "mutation($id: ID!) { approvePage(id: $id, comment: \"Great\") { stage workflowStatus title } }", v.clone()).await;
    assert_eq!(d["approvePage"], json!({"stage": "PUBLISHED", "workflowStatus": "PUBLISHED", "title": "Hello, world"}));

    // Public readers see the page, but not who worked on it.
    let d = env.ok(&public, "{ pages { items { title createdBy history { action } } } }", json!({})).await;
    assert_eq!(d["pages"]["items"][0], json!({"title": "Hello, world", "createdBy": null, "history": null}));
    let d = env.ok(&member, "{ pages(stage: DRAFT) { items { history { action by comment } } } }", json!({})).await;
    let history = d["pages"]["items"][0]["history"].as_array().unwrap().clone();
    assert_eq!(history[0], json!({"action": "APPROVED", "by": "Rita Reviewer", "comment": "Great"}));
    assert_eq!(history.len(), 6); // created, submitted, changes requested, updated, submitted, approved

    // Without workflow, contributors publish directly (Note is out of Carl's scope: widen it).
    let carl = env.ok(&owner, "{ users { id email } }", json!({})).await["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["email"] == "c@example.com")
        .unwrap()["id"]
        .clone();
    let carl_session = env.login_token("c@example.com").await;
    assert!(env.auth.authenticate(&carl_session).await.is_ok());
    env.ok(&owner, "mutation($id: ID!) { updateUser(id: $id, input: {scope: []}) { scope } }", json!({"id": carl})).await;
    // The scope change signed Carl out.
    assert!(env.auth.authenticate(&carl_session).await.is_err());
    let contributor = env.login("c@example.com").await;
    let d = env.ok(&contributor, r#"mutation { createNote(data: {title: "Quick"}) { id } }"#, json!({})).await;
    let note = json!({"id": d["createNote"]["id"]});
    let d = env.ok(&contributor, "mutation($id: ID!) { publishNote(id: $id) { workflowStatus } }", note.clone()).await;
    assert_eq!(d["publishNote"]["workflowStatus"], "PUBLISHED");
    assert_eq!(env.code(&contributor, "mutation($id: ID!) { deleteNote(id: $id) }", note).await, "FORBIDDEN");

    // API tokens: role-limited, shown once.
    let d = env
        .ok(&owner, r#"mutation { createApiToken(name: "Site build", role: MEMBER, expiresInDays: 30) { token apiToken { id role } } }"#, json!({}))
        .await;
    let token = d["createApiToken"]["token"].as_str().unwrap();
    let as_token = env.principal(token).await;
    env.ok(&as_token, "{ pages(stage: DRAFT) { total } }", json!({})).await;
    assert_eq!(env.code(&as_token, r#"mutation { createNote(data: {title: "x"}) { id } }"#, json!({})).await, "FORBIDDEN");
    assert_eq!(env.ok(&as_token, "{ me { name } }", json!({})).await["me"], Value::Null);
    // Explicit nulls for optional arguments behave like omitted ones.
    let d = env
        .ok(&owner, "mutation($d: Int) { createApiToken(name: \"No expiry\", role: MEMBER, expiresInDays: $d) { apiToken { expiresAt } } }", json!({"d": null}))
        .await;
    assert_eq!(d["createApiToken"]["apiToken"]["expiresAt"], Value::Null);
    env.ok(&owner, "{ pages(first: null, skip: null, stage: null) { total } }", json!({})).await;
    let admin_token = r#"mutation { createApiToken(name: "x", role: OWNER) { token } }"#;
    let admin = env.ok(&owner, "{ users { id role } }", json!({})).await; // sanity: owner can list
    assert!(admin["users"].as_array().unwrap().iter().any(|u| u["role"] == "OWNER"));
    assert_eq!(env.code(&contributor, admin_token, json!({})).await, "FORBIDDEN");
}
