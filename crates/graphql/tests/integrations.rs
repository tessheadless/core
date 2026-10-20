// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! GraphQL API of the translation and webhook services.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::routing::post;
use serde_json::{Value, json};
use tess_core::memory::MemoryStorage;
use tess_core::secrets::SecretBox;
use tess_core::{Actor, Role, Storage};
use tess_graphql::ids::{IdCodec, IdConfig};
use tess_graphql::{GraphqlService, Principal, SchemaConfig, Services, WorkflowsInfo};

fn codec() -> IdCodec {
    IdCodec::new(IdConfig {
        secret: vec![1; 32],
        previous_secrets: vec![],
        ref_secret: vec![2; 32],
        ttl: Duration::from_secs(3600),
    })
    .unwrap()
}

async fn service() -> GraphqlService {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let secrets = SecretBox::new(&[5u8; 32]).unwrap();
    let ids = Arc::new(codec());
    let refs = ids.clone();
    let webhooks = tess_webhooks::WebhookService::new(
        storage.clone(),
        secrets.clone(),
        Arc::new(move |ct, id| ids.encode(ct, id)),
        Arc::new(move |ct, id| refs.stable_ref(ct, id)),
        tess_webhooks::WebhookConfig {
            inline: true,
            ..Default::default()
        },
    );
    let services = Services {
        i18n: Some(Arc::new(tess_i18n::I18nService::new(
            storage.clone(),
            secrets,
            Default::default(),
        ))),
        webhooks: Some(Arc::new(webhooks)),
        workflows: Some(WorkflowsInfo {
            provider: "n8n".into(),
            editor_url: "/n8n/".into(),
        }),
        ..Default::default()
    };
    let svc = GraphqlService::with_services(storage, codec(), SchemaConfig::default(), services);
    svc.install().await.unwrap();
    svc
}

async fn run(svc: &GraphqlService, who: &Principal, query: &str, vars: Value) -> Value {
    let req =
        async_graphql::Request::new(query).variables(async_graphql::Variables::from_json(vars));
    serde_json::to_value(svc.execute(req, who.clone()).await).unwrap()
}

async fn ok(svc: &GraphqlService, who: &Principal, query: &str, vars: Value) -> Value {
    let res = run(svc, who, query, vars).await;
    assert!(res.get("errors").is_none(), "unexpected errors: {res:#}");
    res["data"].clone()
}

fn code(res: &Value) -> &str {
    res["errors"][0]["extensions"]["code"]
        .as_str()
        .unwrap_or_default()
}

fn user(role: Role) -> Principal {
    Principal::User(Actor {
        id: None,
        name: format!("{role:?}"),
        role,
        scope: vec![],
    })
}

#[tokio::test]
async fn translations_api() {
    let svc = service().await;
    let admin = Principal::User(Actor::system());
    let contributor = user(Role::Contributor);
    let reviewer = user(Role::Reviewer);

    assert_eq!(
        code(
            &run(
                &svc,
                &Principal::Public,
                "{ translationLanguages { tag } }",
                json!({})
            )
            .await
        ),
        "FORBIDDEN"
    );
    let res = run(
        &svc,
        &contributor,
        r#"mutation { addTranslationLanguage(tag: "en") { tag } }"#,
        json!({}),
    )
    .await;
    assert_eq!(code(&res), "FORBIDDEN");
    ok(&svc, &admin, r#"mutation { a: addTranslationLanguage(tag: "en", name: "English") { tag base } b: addTranslationLanguage(tag: "de_de") { tag base } }"#, json!({})).await;

    let created = ok(
        &svc,
        &contributor,
        r#"mutation($i: TranslationKeyInput!) { createTranslationKey(input: $i) { id name namespace tags translations { language text state } } }"#,
        json!({"i": {"name": "cart.items", "namespace": "shop", "plural": true, "tags": ["cart"],
                     "translations": [{"language": "en", "text": "{count, plural, one {# item} other {# items}}"}]}}),
    )
    .await;
    let key = &created["createTranslationKey"];
    assert_eq!(key["translations"][0]["state"], "TRANSLATED");
    let id = key["id"].as_str().unwrap();

    let set = r#"mutation($id: ID!, $t: String, $s: TranslationState) { setTranslation(keyId: $id, language: "de-DE", text: $t, state: $s) { translations { language state } } }"#;
    ok(
        &svc,
        &contributor,
        set,
        json!({"id": id, "t": "{count, plural, one {# Artikel} other {# Artikel}}"}),
    )
    .await;
    assert_eq!(
        code(&run(&svc, &contributor, set, json!({"id": id, "s": "REVIEWED"})).await),
        "FORBIDDEN"
    );
    let reviewed = ok(&svc, &reviewer, set, json!({"id": id, "s": "REVIEWED"})).await;
    assert_eq!(
        reviewed["setTranslation"]["translations"][0]["state"],
        "REVIEWED"
    );

    let page = ok(
        &svc,
        &reviewer,
        r#"{ translationKeys(filter: {state: REVIEWED, language: "de-DE"}) { total items { name } } translationStats { language translated reviewed } translationNamespaces }"#,
        json!({}),
    )
    .await;
    assert_eq!(page["translationKeys"]["total"], 1);
    assert_eq!(page["translationNamespaces"], json!(["shop"]));

    let files = ok(
        &svc,
        &contributor,
        r#"{ exportTranslations(input: {format: I18NEXT, languages: ["de-DE"]}) { path contentType content } }"#,
        json!({}),
    )
    .await;
    let file = &files["exportTranslations"][0];
    assert_eq!(file["path"], "de-DE/shop.json");
    let content: Value = serde_json::from_str(file["content"].as_str().unwrap()).unwrap();
    assert_eq!(content["cart"]["items_one"], "{{count}} Artikel");

    let report = ok(
        &svc,
        &contributor,
        r#"mutation($i: TranslationImportInput!) { importTranslations(input: $i) { keysCreated translationsCreated } }"#,
        json!({"i": {"format": "PO", "content": "msgid \"hello\"\nmsgstr \"Hallo\"\n", "language": "de-DE"}}),
    )
    .await;
    assert_eq!(
        report["importTranslations"],
        json!({"keysCreated": 1, "translationsCreated": 1})
    );

    let ids = ok(
        &svc,
        &contributor,
        "{ translationKeys { items { id } } }",
        json!({}),
    )
    .await;
    let ids: Vec<Value> = ids["translationKeys"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["id"].clone())
        .collect();
    let mv =
        "mutation($ids: [ID!]!, $ns: String!) { moveTranslationKeys(ids: $ids, namespace: $ns) }";
    assert_eq!(
        code(&run(&svc, &reviewer, mv, json!({"ids": ids, "ns": "web"})).await),
        "FORBIDDEN"
    );
    let moved = ok(&svc, &contributor, mv, json!({"ids": ids, "ns": "web"})).await;
    assert_eq!(moved["moveTranslationKeys"], 2);
    let ns = ok(&svc, &contributor, "{ translationNamespaces }", json!({})).await;
    assert_eq!(ns["translationNamespaces"], json!(["web"]));

    // Delivery targets: admin only, secrets write-only.
    let target = r#"mutation($i: TranslationDeliveryTargetInput!) { createTranslationDeliveryTarget(input: $i) { id kind hasSecret formats azureAuth } }"#;
    let input = json!({"i": {"name": "R2", "kind": "S3", "provider": "r2", "bucket": "i18n-bucket",
        "endpoint": "https://acc.r2.cloudflarestorage.com", "region": "auto", "accessKeyId": "AK", "secret": "SK",
        "formats": ["JSON", "ANDROID_XML"]}});
    assert_eq!(
        code(&run(&svc, &contributor, target, input.clone()).await),
        "FORBIDDEN"
    );
    let t = ok(&svc, &admin, target, input).await;
    assert_eq!(t["createTranslationDeliveryTarget"]["hasSecret"], true);
    assert_eq!(
        t["createTranslationDeliveryTarget"]["formats"],
        json!(["JSON", "ANDROID_XML"])
    );
    let formats = ok(
        &svc,
        &contributor,
        "{ translationFormats { format slug importable } workflowEditor { provider url } }",
        json!({}),
    )
    .await;
    assert_eq!(formats["translationFormats"].as_array().unwrap().len(), 11);
    assert_eq!(formats["workflowEditor"]["url"], "/n8n/");
}

#[tokio::test]
async fn webhooks_api_and_events() {
    let svc = service().await;
    let admin = Principal::User(Actor::system());
    let received: Arc<Mutex<Vec<Value>>> = Arc::default();
    let sink = received.clone();
    let app = axum::Router::new().route(
        "/hook",
        post(move |body: String| {
            let sink = sink.clone();
            async move {
                sink.lock()
                    .unwrap()
                    .push(serde_json::from_str(&body).unwrap());
                "ok"
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let create = r#"mutation($i: WebhookInput!) { createWebhook(input: $i) { secret webhook { id events headers managedBy } } }"#;
    let input = json!({"i": {"name": "n8n", "url": url, "events": ["entry.*", "contentType.*", "i18n.*"],
        "headers": [{"name": "Authorization", "value": "Bearer x"}], "managedBy": "n8n"}});
    assert_eq!(
        code(&run(&svc, &user(Role::Contributor), create, input.clone()).await),
        "FORBIDDEN"
    );
    let res = ok(&svc, &admin, create, input).await;
    let hook = &res["createWebhook"]["webhook"];
    assert_eq!(
        hook["headers"],
        json!(["Authorization"]),
        "header values are write-only"
    );
    assert!(res["createWebhook"]["secret"].as_str().unwrap().len() >= 32);
    let id = hook["id"].as_str().unwrap().to_string();

    ok(&svc, &admin, r#"mutation { createContentType(input: {apiId: "Note", fields: [{name: "title", type: TEXT}, {name: "pin", type: PASSWORD}]}) { apiId } }"#, json!({})).await;
    ok(
        &svc,
        &admin,
        r#"mutation { createNote(data: {title: "Hi", pin: "12345678"}) { id } }"#,
        json!({}),
    )
    .await;
    ok(
        &svc,
        &admin,
        r#"mutation { addTranslationLanguage(tag: "en") { tag } }"#,
        json!({}),
    )
    .await;

    // Deliveries are inline in this configuration.
    let got = received.lock().unwrap().clone();
    let events: Vec<&str> = got.iter().map(|b| b["event"].as_str().unwrap()).collect();
    assert_eq!(
        events,
        ["contentType.create", "entry.create", "i18n.language.create"]
    );
    assert_eq!(
        got[1]["data"]["entry"]["draft"],
        json!({"title": "Hi"}),
        "passwords never leave"
    );
    let entry_id = got[1]["data"]["entry"]["id"].as_str().unwrap();
    let fetched = ok(
        &svc,
        &admin,
        "query($id: ID!) { note(id: $id, stage: DRAFT) { title } }",
        json!({"id": entry_id}),
    )
    .await;
    assert_eq!(
        fetched["note"]["title"], "Hi",
        "payload ids work with the API"
    );

    let log = ok(
        &svc,
        &admin,
        "query($id: ID!) { webhookDeliveries(id: $id) { event status attempts } webhookEvents }",
        json!({"id": id}),
    )
    .await;
    assert_eq!(log["webhookDeliveries"].as_array().unwrap().len(), 3);
    assert_eq!(log["webhookDeliveries"][0]["status"], "SUCCESS");
    assert!(
        log["webhookEvents"]
            .as_array()
            .unwrap()
            .contains(&json!("i18n.publish"))
    );
    let ping = ok(
        &svc,
        &admin,
        "mutation($id: ID!) { testWebhook(id: $id) { status event } }",
        json!({"id": id}),
    )
    .await;
    assert_eq!(
        ping["testWebhook"],
        json!({"status": "SUCCESS", "event": "webhook.ping"})
    );
    ok(
        &svc,
        &admin,
        "mutation($id: ID!) { deleteWebhook(id: $id) }",
        json!({"id": id}),
    )
    .await;
    assert_eq!(
        ok(&svc, &admin, "{ webhooks { id } }", json!({})).await["webhooks"],
        json!([])
    );
}

#[tokio::test]
async fn commercial_license_hides_the_workflow_editor() {
    let workflows = || {
        Some(WorkflowsInfo {
            provider: "n8n".into(),
            editor_url: "/n8n/".into(),
        })
    };
    let query = "{ license workflowEditor { url } }";
    for (license, editor) in [
        (tess_graphql::License::Community, json!({"url": "/n8n/"})),
        (tess_graphql::License::Commercial, Value::Null),
    ] {
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let services = Services {
            workflows: workflows(),
            license,
            ..Default::default()
        };
        let svc =
            GraphqlService::with_services(storage, codec(), SchemaConfig::default(), services);
        svc.install().await.unwrap();
        let admin = Principal::User(Actor::system());
        let res = ok(&svc, &admin, query, json!({})).await;
        assert_eq!(
            res,
            json!({"license": license.as_str(), "workflowEditor": editor})
        );
        assert_eq!(
            code(&run(&svc, &Principal::Public, query, json!({})).await),
            "FORBIDDEN"
        );
    }
}
