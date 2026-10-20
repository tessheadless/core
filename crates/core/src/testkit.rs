// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Conformance suite every [`Storage`] backend must pass.
//!
//! Each query is run through the backend and through the reference
//! [`execute_in_memory`] evaluator; both must return the same ids in the same
//! order and the same total. Backends call [`run_conformance`] from their own
//! tests against a fresh (or disposable) database.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::model::{ContentTypeDef, FieldDef, FieldType, Stage};
use crate::query::{Direction, EntryQuery, Filter, Op, SortKey, execute_in_memory};
use crate::{Actor, Cms, CmsError, Role, Storage, WorkflowStatus};

const AUTHOR: &str = "TkAuthor";
const POST: &str = "TkPost";

fn field(name: &str, field_type: FieldType, required: bool) -> FieldDef {
    FieldDef {
        name: name.into(),
        field_type,
        required,
        description: None,
    }
}

fn data(v: Value) -> serde_json::Map<String, Value> {
    v.as_object().cloned().expect("object")
}

pub async fn run_conformance(storage: Arc<dyn Storage>) {
    storage.migrate().await.expect("migrate");
    storage.migrate().await.expect("migrate is idempotent");
    // Leftovers of a previous run against a persistent database.
    for ct in [POST, AUTHOR] {
        let _ = storage.delete_content_type(ct).await;
    }
    let cms = Cms::new(storage.clone());

    content_type_lifecycle(&cms).await;
    let author_id = seed(&cms).await;
    query_equivalence(&cms).await;
    draft_and_publish(&cms).await;
    relations_and_deletes(&cms, author_id).await;
    settings(storage.as_ref()).await;
    components_round_trip(&cms).await;
}

/// New field types and component data survive the backend unchanged.
async fn components_round_trip(cms: &Cms) {
    let sys = Actor::system();
    let _ = cms.delete_content_type("TkPage", &sys).await;
    let _ = cms.delete_component("TkBlock", &sys).await;
    cms.create_component(
        crate::ComponentDef {
            api_id: "TkBlock".into(),
            description: None,
            fields: vec![
                field("heading", FieldType::text(), true),
                field("body", FieldType::Markdown, false),
            ],
        },
        &sys,
    )
    .await
    .expect("create component");
    cms.create_content_type(
        ContentTypeDef {
            api_id: "TkPage".into(),
            plural_api_id: None,
            description: None,
            workflow: false,
            fields: vec![
                field("day", FieldType::Date, false),
                field("code", FieldType::Uuid, false),
                field("secret", FieldType::Password { min_length: 8 }, false),
                field(
                    "hero",
                    FieldType::Component {
                        component: "TkBlock".into(),
                        repeatable: false,
                    },
                    false,
                ),
                field(
                    "zone",
                    FieldType::DynamicZone {
                        components: vec!["TkBlock".into()],
                    },
                    false,
                ),
            ],
        },
        &sys,
    )
    .await
    .expect("create page type");
    let e = cms
        .create_entry(
            "TkPage",
            data(json!({
                "day": "2026-10-18", "secret": "a long secret",
                "hero": {"heading": "Hello", "body": "**bold** <Callout />"},
                "zone": [{"__component": "TkBlock", "heading": "One"}, {"__component": "TkBlock", "heading": "Two"}]
            })),
            &sys,
        )
        .await
        .expect("create page");
    let stored = cms
        .storage()
        .get_entry("TkPage", e.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.draft, e.draft, "component data round-trips");
    assert!(
        cms.verify_password("TkPage", e.id, "secret", Stage::Draft, "a long secret")
            .await
            .unwrap()
    );
    let mut q = EntryQuery::new(Stage::Draft);
    q.filter = Some(Filter::cmp("day", Op::Gte, "2026-01-01"));
    assert_eq!(cms.query_entries("TkPage", &q).await.unwrap().total, 1);
    cms.delete_content_type("TkPage", &sys).await.unwrap();
    cms.delete_component("TkBlock", &sys).await.unwrap();
    assert!(cms.components().await.unwrap().is_empty());
}

async fn settings(storage: &dyn Storage) {
    for (k, _) in storage.list_settings("tk/").await.unwrap() {
        storage.delete_setting(&k).await.unwrap();
    }
    assert_eq!(storage.get_setting("tk/a").await.unwrap(), None);
    let v = json!({"enabled": true, "n": 1.5, "nested": {"list": [1, "x", null]}});
    storage.put_setting("tk/b", &v).await.unwrap();
    storage.put_setting("tk/a", &json!("first")).await.unwrap();
    storage.put_setting("tk/a", &json!("second")).await.unwrap();
    storage.put_setting("tkz", &json!(0)).await.unwrap();
    assert_eq!(storage.get_setting("tk/b").await.unwrap(), Some(v.clone()));
    let listed = storage.list_settings("tk/").await.unwrap();
    assert_eq!(
        listed,
        vec![
            ("tk/a".to_string(), json!("second")),
            ("tk/b".to_string(), v)
        ]
    );
    assert!(storage.delete_setting("tk/a").await.unwrap());
    assert!(!storage.delete_setting("tk/a").await.unwrap());
    assert!(storage.delete_setting("tkz").await.unwrap());
    assert!(storage.delete_setting("tk/b").await.unwrap());
    assert!(storage.list_settings("tk/").await.unwrap().is_empty());
}

async fn content_type_lifecycle(cms: &Cms) {
    let author = cms
        .create_content_type(
            ContentTypeDef {
                workflow: false,
                api_id: AUTHOR.into(),
                plural_api_id: None,
                description: Some("people".into()),
                fields: vec![field("name", FieldType::text(), true)],
            },
            &Actor::system(),
        )
        .await
        .expect("create author");
    assert_eq!(author.plural_api_id, "tkAuthors");

    let dup = cms
        .create_content_type(
            ContentTypeDef {
                workflow: false,
                api_id: AUTHOR.into(),
                plural_api_id: None,
                description: None,
                fields: vec![field("x", FieldType::Int, false)],
            },
            &Actor::system(),
        )
        .await;
    assert!(
        matches!(dup, Err(CmsError::Conflict(_))),
        "duplicate apiId: {dup:?}"
    );

    cms.create_content_type(
        ContentTypeDef {
            workflow: false,
            api_id: POST.into(),
            plural_api_id: Some("tkPostList".into()),
            description: None,
            fields: vec![
                field("title", FieldType::text(), true),
                field("views", FieldType::Int, false),
                field("rating", FieldType::Float, false),
                field("featured", FieldType::Boolean, false),
                field("date", FieldType::DateTime, false),
                field(
                    "status",
                    FieldType::Enum {
                        values: vec!["DRAFTED".into(), "LIVE".into()],
                    },
                    false,
                ),
                field("meta", FieldType::Json, false),
                field(
                    "author",
                    FieldType::Relation {
                        target: AUTHOR.into(),
                        many: false,
                    },
                    false,
                ),
            ],
        },
        &Actor::system(),
    )
    .await
    .expect("create post");

    let loaded = cms.content_type(POST).await.expect("get post type");
    assert_eq!(loaded.fields.len(), 8);
    assert_eq!(loaded.plural_api_id, "tkPostList");
    let listed: Vec<_> = cms
        .content_types()
        .await
        .unwrap()
        .into_iter()
        .map(|c| c.api_id)
        .collect();
    assert!(listed.contains(&POST.to_string()) && listed.contains(&AUTHOR.to_string()));
}

async fn seed(cms: &Cms) -> uuid::Uuid {
    let author = cms
        .create_entry(AUTHOR, data(json!({"name": "Writer A"})), &Actor::system())
        .await
        .unwrap();
    let rows = [
        json!({"title": "Alpha", "views": 10, "rating": 4.5, "featured": true, "date": "2026-01-01T10:00:00Z", "status": "LIVE", "meta": {"k": 1}, "author": author.id.to_string()}),
        json!({"title": "beta", "views": 3, "rating": 2.0, "featured": false, "date": "2026-03-01T10:00:00+02:00", "status": "DRAFTED"}),
        json!({"title": "Gamma ray", "views": 10, "featured": true, "status": "LIVE"}),
        json!({"title": "Delta", "rating": 3, "date": "2025-12-31T23:59:59.999Z"}),
        json!({"title": "alphabet soup", "views": 0, "rating": -1.25, "featured": false}),
        json!({"title": "Zeta", "views": 1000000, "status": "DRAFTED", "author": author.id.to_string()}),
    ];
    for row in rows {
        cms.create_entry(POST, data(row), &Actor::system())
            .await
            .expect("create post entry");
    }
    let invalid = cms
        .create_entry(POST, data(json!({"views": 1})), &Actor::system())
        .await;
    assert!(
        matches!(invalid, Err(CmsError::Validation(_))),
        "required field: {invalid:?}"
    );
    let invalid = cms
        .create_entry(
            POST,
            data(json!({"title": "x", "views": "many"})),
            &Actor::system(),
        )
        .await;
    assert!(
        matches!(invalid, Err(CmsError::Validation(_))),
        "type check: {invalid:?}"
    );
    author.id
}

async fn query_equivalence(cms: &Cms) {
    let storage = cms.storage();
    let mut all_q = EntryQuery::new(Stage::Draft);
    all_q.limit = 10_000;
    let all = storage.query_entries(POST, &all_q).await.unwrap().items;
    assert_eq!(all.len(), 6);

    let filters = vec![
        None,
        Some(Filter::cmp("title", Op::Eq, "Alpha")),
        Some(Filter::cmp("title", Op::Ne, "Alpha")),
        Some(Filter::cmp("views", Op::Eq, 10)),
        Some(Filter::cmp("views", Op::Eq, 10.0)),
        Some(Filter::cmp("views", Op::Gt, 3)),
        Some(Filter::cmp("views", Op::Gte, 3)),
        Some(Filter::cmp("views", Op::Lt, 10)),
        Some(Filter::cmp("views", Op::Lte, 10)),
        Some(Filter::cmp("views", Op::Eq, Value::Null)),
        Some(Filter::cmp("views", Op::Ne, Value::Null)),
        Some(Filter::cmp("views", Op::Ne, 10)),
        Some(Filter::cmp("rating", Op::Gt, 2)),
        Some(Filter::cmp("rating", Op::Lte, 2.0)),
        Some(Filter::cmp("featured", Op::Eq, true)),
        Some(Filter::cmp("featured", Op::Ne, true)),
        Some(Filter::cmp("date", Op::Gt, "2026-01-01T00:00:00.000Z")),
        Some(Filter::cmp("status", Op::In, json!(["LIVE", "DRAFTED"]))),
        Some(Filter::cmp("status", Op::NotIn, json!(["LIVE"]))),
        Some(Filter::cmp("status", Op::In, json!(["LIVE", null]))),
        Some(Filter::cmp("title", Op::Contains, "ph")),
        Some(Filter::cmp("title", Op::Contains, "%")),
        Some(Filter::cmp("title", Op::Contains, ".*")),
        Some(Filter::cmp("title", Op::StartsWith, "alpha")),
        Some(Filter::cmp("title", Op::EndsWith, "a")),
        Some(Filter::cmp("title", Op::Gt, "Delta")),
        Some(Filter::Or(vec![
            Filter::cmp("views", Op::Gt, 100),
            Filter::cmp("rating", Op::Lt, 0),
        ])),
        Some(Filter::Not(Box::new(Filter::cmp("featured", Op::Eq, true)))),
        Some(Filter::And(vec![
            Filter::cmp("featured", Op::Eq, true),
            Filter::Not(Box::new(Filter::cmp("title", Op::StartsWith, "G"))),
        ])),
        Some(Filter::cmp("id", Op::Eq, all[2].id.to_string())),
        Some(Filter::cmp(
            "id",
            Op::In,
            json!([all[0].id.to_string(), all[4].id.to_string()]),
        )),
        Some(Filter::cmp(
            "createdAt",
            Op::Gte,
            crate::model::format_ts(all[0].created_at),
        )),
        Some(Filter::And(vec![])),
        Some(Filter::Or(vec![])),
    ];
    let sorts = vec![
        vec![],
        vec![SortKey {
            field: "views".into(),
            direction: Direction::Desc,
        }],
        vec![SortKey {
            field: "views".into(),
            direction: Direction::Asc,
        }],
        vec![
            SortKey {
                field: "featured".into(),
                direction: Direction::Asc,
            },
            SortKey {
                field: "rating".into(),
                direction: Direction::Desc,
            },
        ],
        vec![SortKey {
            field: "date".into(),
            direction: Direction::Asc,
        }],
        vec![SortKey {
            field: "title".into(),
            direction: Direction::Desc,
        }],
    ];
    for filter in &filters {
        for sort in &sorts {
            for (offset, limit) in [(0, 100), (1, 2), (5, 10)] {
                let q = EntryQuery {
                    stage: Stage::Draft,
                    filter: filter.clone(),
                    sort: sort.clone(),
                    limit,
                    offset,
                };
                let got = storage.query_entries(POST, &q).await.unwrap();
                let want = execute_in_memory(all.clone(), &q);
                let ids = |p: &crate::Page| p.items.iter().map(|e| e.id).collect::<Vec<_>>();
                assert_eq!(got.total, want.total, "total for {filter:?} / {sort:?}");
                assert_eq!(
                    ids(&got),
                    ids(&want),
                    "order for {filter:?} / {sort:?} / {offset},{limit}"
                );
            }
        }
    }
    // Stored data round-trips unchanged.
    let first = all.iter().find(|e| e.draft["title"] == "Alpha").unwrap();
    assert_eq!(first.draft["meta"], json!({"k": 1}));
    assert_eq!(first.draft["rating"], json!(4.5));
    let beta = all.iter().find(|e| e.draft["title"] == "beta").unwrap();
    assert_eq!(beta.draft["date"], json!("2026-03-01T08:00:00.000Z"));
}

async fn draft_and_publish(cms: &Cms) {
    let editor = Actor {
        id: Some(uuid::Uuid::new_v4()),
        name: "Editor Example".into(),
        role: Role::Contributor,
        scope: vec![],
    };
    let entry = cms
        .create_entry(POST, data(json!({"title": "Unpublished"})), &editor)
        .await
        .unwrap();
    let published_q = EntryQuery::new(Stage::Published);
    let before = cms.query_entries(POST, &published_q).await.unwrap().total;
    assert!(
        cms.get_entry(POST, entry.id, Stage::Published)
            .await
            .unwrap()
            .is_none()
    );

    cms.publish_entry(POST, entry.id, &editor).await.unwrap();
    // Workflow metadata round-trips through the backend.
    let stored = cms
        .storage()
        .get_entry(POST, entry.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, WorkflowStatus::Published);
    assert_eq!(
        stored.created_by.as_ref().map(|a| a.name.as_str()),
        Some("Editor Example")
    );
    assert_eq!(stored.created_by.as_ref().and_then(|a| a.id), editor.id);
    assert_eq!(stored.history.len(), 2, "{:?}", stored.history);
    assert_eq!(stored.history[0].by.name, "Editor Example");
    let edited = cms
        .update_entry(
            POST,
            entry.id,
            data(json!({"title": "Edited"})),
            &Actor::system(),
        )
        .await
        .unwrap();
    assert!(edited.updated_at >= edited.created_at);
    let published = cms
        .get_entry(POST, entry.id, Stage::Published)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        published.published.as_ref().unwrap()["title"],
        "Unpublished"
    );
    assert_eq!(published.draft["title"], "Edited");
    assert_eq!(
        cms.query_entries(POST, &published_q).await.unwrap().total,
        before + 1
    );

    let mut q = EntryQuery::new(Stage::Published);
    q.filter = Some(Filter::cmp("title", Op::Eq, "Edited"));
    assert_eq!(cms.query_entries(POST, &q).await.unwrap().total, 0);
    q.stage = Stage::Draft;
    assert_eq!(cms.query_entries(POST, &q).await.unwrap().total, 1);

    let cleared = cms
        .update_entry(POST, entry.id, data(json!({"views": 5})), &Actor::system())
        .await
        .unwrap();
    assert_eq!(cleared.draft["views"], 5);
    let cleared = cms
        .update_entry(
            POST,
            entry.id,
            data(json!({"views": null})),
            &Actor::system(),
        )
        .await
        .unwrap();
    assert!(!cleared.draft.contains_key("views"));

    cms.unpublish_entry(POST, entry.id, &Actor::system())
        .await
        .unwrap();
    assert!(
        cms.get_entry(POST, entry.id, Stage::Published)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        cms.query_entries(POST, &published_q).await.unwrap().total,
        before
    );
    cms.delete_entry(POST, entry.id, &Actor::system())
        .await
        .unwrap();
    assert!(matches!(
        cms.delete_entry(POST, entry.id, &Actor::system()).await,
        Err(CmsError::NotFound(_))
    ));
}

async fn relations_and_deletes(cms: &Cms, author_id: uuid::Uuid) {
    let dangling = cms
        .create_entry(
            POST,
            data(json!({"title": "x", "author": uuid::Uuid::new_v4().to_string()})),
            &Actor::system(),
        )
        .await;
    assert!(
        matches!(dangling, Err(CmsError::Validation(_))),
        "dangling relation: {dangling:?}"
    );

    let mut q = EntryQuery::new(Stage::Draft);
    q.filter = Some(Filter::cmp("author", Op::Eq, author_id.to_string()));
    assert_eq!(cms.query_entries(POST, &q).await.unwrap().total, 2);

    let blocked = cms.delete_content_type(AUTHOR, &Actor::system()).await;
    assert!(
        matches!(blocked, Err(CmsError::Conflict(_))),
        "referenced type: {blocked:?}"
    );

    cms.delete_content_type(POST, &Actor::system())
        .await
        .unwrap();
    cms.delete_content_type(AUTHOR, &Actor::system())
        .await
        .unwrap();
    assert!(
        cms.storage()
            .get_content_type(POST)
            .await
            .unwrap()
            .is_none()
    );
    let mut all = EntryQuery::new(Stage::Draft);
    all.limit = 10_000;
    assert_eq!(
        cms.storage().query_entries(POST, &all).await.unwrap().total,
        0
    );
    assert!(
        cms.storage()
            .get_entry(AUTHOR, author_id)
            .await
            .unwrap()
            .is_none()
    );
}
