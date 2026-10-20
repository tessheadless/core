// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Webhook management API (admin only).

use std::sync::Arc;

use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputObject, InputValue, ResolverContext, TypeRef,
};
use serde_json::{Value, json};
use tess_core::CmsError;
use tess_webhooks::{DeliveryRecord, WebhookInput, WebhookRecord, WebhookService, WebhookUpdate};

use crate::jsonobj::{enum_type, enumerated, json_object, plain, value, values};
use crate::schema::{GqlResult, TypeDef, admin_require, cms_err, json_arg};

fn service<'a>(ctx: &ResolverContext<'a>) -> GqlResult<&'a Arc<WebhookService>> {
    ctx.data::<Arc<WebhookService>>()
}

/// What the API shows: no secret, header names only.
fn webhook_json(w: &WebhookRecord) -> Value {
    json!({
        "id": w.id,
        "name": w.name,
        "url": w.url,
        "enabled": w.enabled,
        "events": w.events,
        "contentTypes": w.content_types,
        "headers": w.headers.iter().map(|h| &h.name).collect::<Vec<_>>(),
        "managedBy": w.managed_by,
        "createdBy": w.created_by,
        "createdAt": w.created_at,
        "updatedAt": w.updated_at,
    })
}

fn delivery_json(d: &DeliveryRecord) -> Value {
    serde_json::to_value(d).unwrap_or_default()
}

fn strings(v: &Value, key: &str) -> Option<Vec<String>> {
    v.get(key)?.as_array().map(|xs| {
        xs.iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect()
    })
}

fn headers(v: &Value) -> Option<Vec<(String, String)>> {
    v.get("headers")?.as_array().map(|xs| {
        xs.iter()
            .filter_map(|h| {
                Some((
                    h.get("name")?.as_str()?.to_string(),
                    h.get("value")?.as_str()?.to_string(),
                ))
            })
            .collect()
    })
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(String::from)
}

fn types() -> Vec<TypeDef> {
    use TypeRef as T;
    let nn = T::named_nn;
    let list = || T::named_nn_list_nn(T::STRING);
    let webhook = json_object(
        "Webhook",
        "An outgoing webhook: Tess POSTs signed JSON events to `url`.",
        vec![
            plain("id", nn(T::ID)),
            plain("name", nn(T::STRING)),
            plain("url", nn(T::STRING)),
            plain("enabled", nn(T::BOOLEAN)),
            plain("events", list()).doc("Subscribed events: `entry.publish`, `entry.*`, `*`, ..."),
            plain("contentTypes", list())
                .doc("Content types the webhook applies to; empty means all."),
            plain("headers", list()).doc("Names of the extra headers (values are write-only)."),
            plain("managedBy", T::named(T::STRING))
                .doc("Set when an integration (e.g. n8n) registered the webhook."),
            plain("createdBy", nn(T::STRING)),
            plain("createdAt", nn("DateTime")),
            plain("updatedAt", nn("DateTime")),
        ],
    );
    let delivery = json_object(
        "WebhookDelivery",
        "Outcome of sending one event to a webhook.",
        vec![
            plain("id", nn(T::ID)),
            plain("webhookId", nn(T::ID)),
            plain("event", nn(T::STRING)),
            enumerated("status", nn("WebhookDeliveryStatus")),
            plain("attempts", nn(T::INT)),
            plain("responseStatus", T::named(T::INT)),
            plain("error", T::named(T::STRING)),
            plain("durationMs", nn(T::INT)),
            plain("createdAt", nn("DateTime")),
            plain("updatedAt", nn("DateTime")),
        ],
    );
    let created = json_object(
        "CreatedWebhook",
        "`secret` signs deliveries (`X-Tess-Signature`); it is shown only once.",
        vec![
            crate::jsonobj::object("webhook", nn("Webhook")),
            plain("secret", nn(T::STRING)),
        ],
    );
    let header = InputObject::new("WebhookHeaderInput")
        .field(InputValue::new("name", nn(T::STRING)))
        .field(InputValue::new("value", nn(T::STRING)));
    let input = InputObject::new("WebhookInput")
        .field(InputValue::new("name", nn(T::STRING)))
        .field(InputValue::new("url", nn(T::STRING)))
        .field(InputValue::new("enabled", T::named(T::BOOLEAN)).default_value(true))
        .field(InputValue::new("events", T::named_nn_list_nn(T::STRING)))
        .field(InputValue::new("contentTypes", T::named_nn_list(T::STRING)))
        .field(InputValue::new(
            "headers",
            T::named_nn_list("WebhookHeaderInput"),
        ))
        .field(
            InputValue::new("secret", T::named(T::STRING))
                .description("Generated when omitted (at least 16 characters)."),
        )
        .field(InputValue::new("managedBy", T::named(T::STRING)));
    let update = InputObject::new("WebhookUpdateInput")
        .description("Omitted fields are left unchanged; `headers` replaces all headers.")
        .field(InputValue::new("name", T::named(T::STRING)))
        .field(InputValue::new("url", T::named(T::STRING)))
        .field(InputValue::new("enabled", T::named(T::BOOLEAN)))
        .field(InputValue::new("events", T::named_nn_list(T::STRING)))
        .field(InputValue::new("contentTypes", T::named_nn_list(T::STRING)))
        .field(InputValue::new(
            "headers",
            T::named_nn_list("WebhookHeaderInput"),
        ));
    vec![
        TypeDef::Object(webhook),
        TypeDef::Object(delivery),
        TypeDef::Object(created),
        TypeDef::Enum(enum_type(
            "WebhookDeliveryStatus",
            ["RETRYING", "SUCCESS", "FAILED"],
        )),
        TypeDef::Input(header),
        TypeDef::Input(input),
        TypeDef::Input(update),
    ]
}

fn id_input() -> InputValue {
    InputValue::new("id", TypeRef::named_nn(TypeRef::ID))
}

fn arg_id(ctx: &ResolverContext<'_>) -> GqlResult<String> {
    Ok(ctx.args.try_get("id")?.string()?.to_string())
}

pub(crate) fn definitions() -> (Vec<Field>, Vec<Field>, Vec<TypeDef>) {
    let list = Field::new("webhooks", TypeRef::named_nn_list_nn("Webhook"), |ctx| {
        FieldFuture::new(async move {
            admin_require(&ctx)?;
            let all = service(&ctx)?.list().await.map_err(cms_err)?;
            Ok(Some(values(all.iter().map(webhook_json))))
        })
    });
    let get = Field::new("webhook", TypeRef::named("Webhook"), |ctx| {
        FieldFuture::new(async move {
            admin_require(&ctx)?;
            match service(&ctx)?.get(&arg_id(&ctx)?).await {
                Ok(w) => Ok(Some(value(webhook_json(&w)))),
                Err(CmsError::NotFound(_)) => Ok(None),
                Err(e) => Err(cms_err(e)),
            }
        })
    })
    .argument(id_input());
    let deliveries = Field::new(
        "webhookDeliveries",
        TypeRef::named_nn_list_nn("WebhookDelivery"),
        |ctx| {
            FieldFuture::new(async move {
                admin_require(&ctx)?;
                let first = ctx
                    .args
                    .get("first")
                    .map(|v| v.i64())
                    .transpose()?
                    .unwrap_or(25)
                    .clamp(1, 100) as usize;
                let all = service(&ctx)?
                    .deliveries(&arg_id(&ctx)?, first)
                    .await
                    .map_err(cms_err)?;
                Ok(Some(values(all.iter().map(delivery_json))))
            })
        },
    )
    .description("Most recent first.")
    .argument(id_input())
    .argument(InputValue::new("first", TypeRef::named(TypeRef::INT)).default_value(25));
    let events = Field::new(
        "webhookEvents",
        TypeRef::named_nn_list_nn(TypeRef::STRING),
        |ctx| {
            FieldFuture::new(async move {
                // Readable by integrations with any token (the n8n trigger lists them).
                crate::schema::require_write(&ctx)?;
                let all = service(&ctx)?.events();
                Ok(Some(FieldValue::list(
                    all.into_iter().map(FieldValue::value),
                )))
            })
        },
    )
    .description("Every event a webhook can subscribe to.");

    let create = Field::new(
        "createWebhook",
        TypeRef::named_nn("CreatedWebhook"),
        |ctx| {
            FieldFuture::new(async move {
                let actor = admin_require(&ctx)?;
                let v = json_arg(&ctx, "input")?;
                let input = WebhookInput {
                    name: str_field(&v, "name").unwrap_or_default(),
                    url: str_field(&v, "url").unwrap_or_default(),
                    enabled: v.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                    events: strings(&v, "events").unwrap_or_default(),
                    content_types: strings(&v, "contentTypes").unwrap_or_default(),
                    headers: headers(&v).unwrap_or_default(),
                    secret: str_field(&v, "secret"),
                    managed_by: str_field(&v, "managedBy"),
                };
                let (w, secret) = service(&ctx)?
                    .create(input, &actor)
                    .await
                    .map_err(cms_err)?;
                Ok(Some(value(
                    json!({ "webhook": webhook_json(&w), "secret": secret }),
                )))
            })
        },
    )
    .argument(InputValue::new("input", TypeRef::named_nn("WebhookInput")));
    let update = Field::new("updateWebhook", TypeRef::named_nn("Webhook"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let v = json_arg(&ctx, "input")?;
            let update = WebhookUpdate {
                name: str_field(&v, "name"),
                url: str_field(&v, "url"),
                enabled: v.get("enabled").and_then(Value::as_bool),
                events: strings(&v, "events"),
                content_types: strings(&v, "contentTypes"),
                headers: headers(&v),
            };
            let w = service(&ctx)?
                .update(&arg_id(&ctx)?, update, &actor)
                .await
                .map_err(cms_err)?;
            Ok(Some(value(webhook_json(&w))))
        })
    })
    .argument(id_input())
    .argument(InputValue::new(
        "input",
        TypeRef::named_nn("WebhookUpdateInput"),
    ));
    let delete = Field::new("deleteWebhook", TypeRef::named_nn(TypeRef::ID), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let id = arg_id(&ctx)?;
            service(&ctx)?.delete(&id, &actor).await.map_err(cms_err)?;
            Ok(Some(FieldValue::value(id)))
        })
    })
    .argument(id_input());
    let rotate = Field::new(
        "rotateWebhookSecret",
        TypeRef::named_nn(TypeRef::STRING),
        |ctx| {
            FieldFuture::new(async move {
                let actor = admin_require(&ctx)?;
                let secret = service(&ctx)?
                    .rotate_secret(&arg_id(&ctx)?, &actor)
                    .await
                    .map_err(cms_err)?;
                Ok(Some(FieldValue::value(secret)))
            })
        },
    )
    .description("Replaces the signing secret and returns the new one (shown once).")
    .argument(id_input());
    let test = Field::new("testWebhook", TypeRef::named_nn("WebhookDelivery"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let d = service(&ctx)?
                .test(&arg_id(&ctx)?, &actor)
                .await
                .map_err(cms_err)?;
            Ok(Some(value(delivery_json(&d))))
        })
    })
    .description("Sends a `webhook.ping` event now and returns the outcome.")
    .argument(id_input());

    (
        vec![list, get, deliveries, events],
        vec![create, update, delete, rotate, test],
        types(),
    )
}
