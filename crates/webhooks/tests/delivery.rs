// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use serde_json::{Value, json};
use tess_core::memory::MemoryStorage;
use tess_core::secrets::SecretBox;
use tess_core::{Actor, Cms, ContentTypeDef, FieldDef, FieldType, Storage};
use tess_webhooks::{
    DeliveryStatus, IdFn, WebhookConfig, WebhookHook, WebhookInput, WebhookService, WebhookUpdate,
    verify_signature,
};

type Received = Arc<Mutex<Vec<(HeaderMap, Value)>>>;

/// Receiver answering 503 to the first `fail_first` requests.
async fn receiver(fail_first: usize) -> (String, Received) {
    let got: Received = Arc::default();
    let sink = got.clone();
    let app = axum::Router::new().route(
        "/hook",
        post(move |headers: HeaderMap, body: String| {
            let sink = sink.clone();
            async move {
                let mut all = sink.lock().unwrap();
                all.push((headers, serde_json::from_str(&body).unwrap()));
                if all.len() <= fail_first {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::OK
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, got)
}

fn service(storage: Arc<dyn Storage>, inline: bool) -> Arc<WebhookService> {
    let ids: IdFn = Arc::new(|ct, id| format!("id:{ct}:{id}"));
    let refs: IdFn = Arc::new(|ct, _| format!("ref:{ct}"));
    let config = WebhookConfig {
        inline,
        retry_delays: vec![Duration::from_millis(20)],
        ..WebhookConfig::default()
    };
    Arc::new(WebhookService::new(
        storage,
        SecretBox::new(&[1u8; 32]).unwrap(),
        ids,
        refs,
        config,
    ))
}

async fn wait_for(got: &Received, n: usize) {
    for _ in 0..200 {
        if got.lock().unwrap().len() >= n {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("expected {n} deliveries, got {}", got.lock().unwrap().len());
}

fn note_type() -> ContentTypeDef {
    let f = |name: &str, field_type| FieldDef {
        name: name.into(),
        field_type,
        required: false,
        description: None,
    };
    ContentTypeDef {
        api_id: "Note".into(),
        plural_api_id: None,
        description: None,
        workflow: false,
        fields: vec![
            f("title", FieldType::text()),
            f("pin", FieldType::Password { min_length: 4 }),
        ],
    }
}

#[tokio::test]
async fn delivers_signed_redacted_events_and_retries() {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let cms = Cms::new(storage.clone());
    let svc = service(storage, false);
    cms.hooks().add(Arc::new(WebhookHook::new(svc.clone())));
    let (url, got) = receiver(1).await;
    let admin = Actor::system();

    let (hook, secret) = svc
        .create(
            WebhookInput {
                name: "n8n".into(),
                url,
                enabled: true,
                events: vec!["entry.*".into(), "contentType.create".into()],
                headers: vec![("X-Api-Key".into(), "k3y".into())],
                ..WebhookInput::default()
            },
            &admin,
        )
        .await
        .unwrap();

    cms.create_content_type(note_type(), &admin).await.unwrap();
    let data = json!({"title": "hello", "pin": "123456"})
        .as_object()
        .unwrap()
        .clone();
    cms.create_entry("Note", data, &admin).await.unwrap();

    // contentType.create succeeds after one 503 retry, entry.create once: 3 requests.
    wait_for(&got, 3).await;
    let all = got.lock().unwrap().clone();
    let events: Vec<_> = all
        .iter()
        .map(|(_, b)| b["event"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        events.iter().filter(|e| *e == "contentType.create").count(),
        2
    );
    let (headers, body) = all
        .iter()
        .find(|(_, b)| b["event"] == "entry.create")
        .unwrap();
    assert_eq!(body["contentType"], "Note");
    assert_eq!(
        body["data"]["entry"]["draft"],
        json!({"title": "hello"}),
        "password redacted"
    );
    assert!(
        body["data"]["entry"]["id"]
            .as_str()
            .unwrap()
            .starts_with("id:Note:")
    );
    assert_eq!(headers["x-api-key"], "k3y");
    assert_eq!(headers["x-tess-event"], "entry.create");
    let raw = serde_json::to_vec(body).unwrap();
    assert!(verify_signature(
        &secret,
        headers["x-tess-signature"].to_str().unwrap(),
        &raw
    ));

    // The log records the retry.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let log = svc.deliveries(&hook.id, 10).await.unwrap();
    assert_eq!(log.len(), 2);
    let ct_delivery = log
        .iter()
        .find(|d| d.event == "contentType.create")
        .unwrap();
    assert_eq!(
        (ct_delivery.status, ct_delivery.attempts),
        (DeliveryStatus::Success, 2)
    );

    // Disabled webhooks receive nothing.
    svc.update(
        &hook.id,
        WebhookUpdate {
            enabled: Some(false),
            ..Default::default()
        },
        &admin,
    )
    .await
    .unwrap();
    cms.create_entry(
        "Note",
        json!({"title": "x"}).as_object().unwrap().clone(),
        &admin,
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(got.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn validation_permissions_and_ping() {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let svc = service(storage, true);
    let (url, got) = receiver(0).await;
    let admin = Actor::system();
    let input = |url: &str, events: Vec<&str>| WebhookInput {
        name: "x".into(),
        url: url.into(),
        enabled: true,
        events: events.into_iter().map(String::from).collect(),
        ..WebhookInput::default()
    };
    assert!(
        svc.create(input("ftp://h/x", vec!["*"]), &admin)
            .await
            .is_err()
    );
    assert!(
        svc.create(input("http://user:pw@h/x", vec!["*"]), &admin)
            .await
            .is_err()
    );
    assert!(svc.create(input(&url, vec![]), &admin).await.is_err());
    assert!(
        svc.create(input(&url, vec!["entry.**"]), &admin)
            .await
            .is_err()
    );
    let member = Actor {
        id: None,
        name: "m".into(),
        role: tess_core::Role::Contributor,
        scope: vec![],
    };
    assert!(svc.create(input(&url, vec!["*"]), &member).await.is_err());

    let (hook, _) = svc.create(input(&url, vec!["*"]), &admin).await.unwrap();
    let d = svc.test(&hook.id, &admin).await.unwrap();
    assert_eq!(d.status, DeliveryStatus::Success);
    assert_eq!(got.lock().unwrap()[0].1["event"], "webhook.ping");
    assert!(svc.events().contains(&"entry.publish".to_string()));

    svc.delete(&hook.id, &admin).await.unwrap();
    assert!(svc.deliveries(&hook.id, 10).await.unwrap().is_empty());
    assert!(svc.get(&hook.id).await.is_err());
}
