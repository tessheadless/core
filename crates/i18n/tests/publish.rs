// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, RawQuery};
use axum::http::HeaderMap;
use axum::routing::put;
use serde_json::Value;
use tess_core::Actor;
use tess_core::memory::MemoryStorage;
use tess_core::secrets::SecretBox;
use tess_i18n::{
    AzureAuth, DeliveryInput, DeliveryUpdate, Format, I18nConfig, I18nService, KeyInput, StorageKind, TranslationState,
};

type Puts = Arc<Mutex<Vec<(String, Option<String>, String, String)>>>;

/// Fake Azure Blob endpoint (Azurite-style path: /{account}/{container}/{blob}).
async fn fake_azure() -> (String, Puts) {
    let puts: Puts = Arc::default();
    let sink = puts.clone();
    let app = axum::Router::new().route(
        "/devacct/{*path}",
        put(move |Path(path): Path<String>, RawQuery(q): RawQuery, headers: HeaderMap, body: String| {
            let sink = sink.clone();
            async move {
                let blob_type = headers["x-ms-blob-type"].to_str().unwrap().to_string();
                sink.lock().unwrap().push((path, q, blob_type, body));
                axum::http::StatusCode::CREATED
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/devacct", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, puts)
}

#[tokio::test]
async fn publishes_to_azure_and_auto_publishes() {
    let config = I18nConfig { inline: false, debounce: Duration::from_millis(50) };
    let svc = Arc::new(I18nService::new(Arc::new(MemoryStorage::new()), SecretBox::new(&[9u8; 32]).unwrap(), config));
    let admin = Actor::system();
    svc.add_language("en", "English", true, &admin).await.unwrap();
    svc.add_language("fr", "French", false, &admin).await.unwrap();
    let key = KeyInput { name: "hello".into(), translations: vec![("en".into(), "Hello".into())], ..Default::default() };
    let k = svc.create_key(key, &admin).await.unwrap();

    let (endpoint, puts) = fake_azure().await;
    let input = DeliveryInput {
        name: "Azure CDN".into(),
        enabled: true,
        kind: StorageKind::AzureBlob,
        provider: "azure".into(),
        bucket: "i18n".into(),
        endpoint: Some(endpoint),
        region: None,
        force_path_style: false,
        access_key_id: Some("devacct".into()),
        secret: Some("?sv=2022&sig=abc".into()),
        azure_auth: AzureAuth::Sas,
        path_prefix: "/app/v1/".into(),
        public_url: Some("https://cdn.example.com/i18n/app/v1".into()),
        formats: vec![Format::Json, Format::I18next],
        languages: vec![],
        namespaces: vec![],
        states: TranslationState::ALL.to_vec(),
        fallback_to_base: false,
        auto_publish: false,
    };
    let mut bad = input.clone();
    bad.secret = None;
    assert!(svc.create_target(bad, &admin).await.is_err(), "Azure needs a secret");
    let t = svc.create_target(input, &admin).await.unwrap();
    assert!(t.secret.as_deref().unwrap().starts_with("v1:"), "sealed at rest");

    let reports = svc.publish(Some(&t.id), &admin).await.unwrap();
    assert!(reports[0].ok, "{:?}", reports[0].error);
    assert_eq!(reports[0].files, 5); // 2 json + 2 i18next + manifest
    let mut paths: Vec<String> = puts.lock().unwrap().iter().map(|p| p.0.clone()).collect();
    paths.sort();
    assert_eq!(
        paths,
        [
            "i18n/app/v1/i18next/en/translation.json",
            "i18n/app/v1/i18next/fr/translation.json",
            "i18n/app/v1/json/en.json",
            "i18n/app/v1/json/fr.json",
            "i18n/app/v1/manifest.json",
        ]
    );
    {
        let all = puts.lock().unwrap();
        assert!(all.iter().all(|p| p.1.as_deref() == Some("sv=2022&sig=abc") && p.2 == "BlockBlob"));
        let manifest: Value = serde_json::from_str(&all.iter().find(|p| p.0.ends_with("manifest.json")).unwrap().3).unwrap();
        assert_eq!(manifest["files"]["json"][0], "json/en.json");
    }
    assert_eq!(svc.target(&t.id).await.unwrap().last_file_count, 5);

    // Auto-publish: a burst of changes publishes once, after the debounce.
    svc.update_target(&t.id, DeliveryUpdate { auto_publish: Some(true), formats: Some(vec![Format::Json]), ..Default::default() }, &admin)
        .await
        .unwrap();
    puts.lock().unwrap().clear();
    svc.set_translation(&k.id(), "fr", Some("Bonjour".into()), None, &admin).await.unwrap();
    svc.set_translation(&k.id(), "fr", Some("Salut".into()), None, &admin).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let all = puts.lock().unwrap();
    assert_eq!(all.len(), 3, "one publish: en, fr, manifest");
    let fr = all.iter().find(|p| p.0.ends_with("json/fr.json")).unwrap();
    assert!(fr.3.contains("Salut"));
}
