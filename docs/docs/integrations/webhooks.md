---
sidebar_position: 2
title: Webhooks
---

# Webhooks

Admins register webhooks (admin UI → **Webhooks**, or the GraphQL API): Tess
then POSTs a signed JSON event to each subscribed URL when content, models or
translations change. The [n8n integration](n8n.md) registers its webhooks by
itself.

## Events

| Event | When |
|---|---|
| `entry.create`, `entry.update`, `entry.delete` | An entry draft was created, changed or deleted. |
| `entry.publish`, `entry.unpublish` | An entry was published or unpublished. |
| `entry.submit`, `entry.approve`, `entry.requestChanges` | Review workflow steps. |
| `contentType.create`, `contentType.update`, `contentType.delete` | A content model changed. |
| `component.create`, `component.update`, `component.delete` | A component changed. |
| `i18n.language.*`, `i18n.key.*`, `i18n.translation.update`, `i18n.import`, `i18n.publish` | [Translations](translations.md) changed. |
| `webhook.ping` | The **Test** button / `testWebhook`. |

A webhook subscribes to exact names, prefixes (`entry.*`) or everything (`*`),
optionally for some content types only (events without a content type, like
components and translations, are not filtered).

## Payload

```json
{
  "id": "6f3c…",
  "event": "entry.publish",
  "createdAt": "2026-10-20T08:00:00.000Z",
  "contentType": "Post",
  "actor": { "name": "Ada" },
  "data": {
    "entry": {
      "id": "<opaque id, usable with the API>",
      "ref": "<stable ref>",
      "draft": { "title": "Hello" },
      "published": { "title": "Hello" },
      "createdAt": "…", "updatedAt": "…", "publishedAt": "…",
      "workflowStatus": "PUBLISHED", "createdBy": "Ada", "updatedBy": "Ada"
    },
    "previous": { "…": "the entry before the change" }
  }
}
```

Model events carry the definition in `data` (`contentType.*`, `component.*`).
`PASSWORD` fields are never sent. Opaque ids expire (`TESS_ID_TTL`): use them
soon, or match entries by `ref`.

## Verifying requests

Each request has the headers `X-Tess-Event`, `X-Tess-Delivery`,
`X-Tess-Webhook` and

```text
X-Tess-Signature: t=<unix seconds>,v1=<hex HMAC-SHA256(secret, "<t>.<raw body>")>
```

The signing secret is shown once when the webhook is created (and by **Rotate
secret**). Reject requests whose signature does not match or whose `t` is too
old:

```js
import { createHmac, timingSafeEqual } from "node:crypto";

function verify(secret, header, rawBody, toleranceSeconds = 300) {
  const { t, v1 } = Object.fromEntries(header.split(",").map((p) => p.split("=", 2)));
  if (Math.abs(Date.now() / 1000 - Number(t)) > toleranceSeconds) return false;
  const expected = createHmac("sha256", secret).update(`${t}.`).update(rawBody).digest();
  const given = Buffer.from(v1 ?? "", "hex");
  return given.length === expected.length && timingSafeEqual(given, expected);
}
```

Extra headers (e.g. an API key the receiver expects) can be added; their
values are encrypted at rest and write-only.

## Delivery

- Deliveries run in the background. Network errors, `408`, `429` and `5xx`
  are retried up to `TESS_WEBHOOK_MAX_ATTEMPTS` times (after 10 s, 1 min,
  5 min, 30 min). Other `4xx` answers are final. Redirects are not followed.
- Pending retries live in memory: a restart drops them.
- On AWS Lambda, deliveries happen within the request, with one attempt.
- The last 25 deliveries of each webhook (status, HTTP code, error, attempts,
  duration) are shown under **Deliveries** and by `webhookDeliveries`.
- `TESS_WEBHOOK_ALLOWED_HOSTS` restricts the hosts webhooks may target.

## GraphQL API (Admins)

```graphql
mutation {
  createWebhook(input: {
    name: "Rebuild site", url: "https://ci.example.com/hooks/tess",
    events: ["entry.publish", "entry.unpublish"], contentTypes: ["Post"],
    headers: [{ name: "Authorization", value: "Bearer …" }]
  }) { secret webhook { id } }
}
query { webhooks { id name url events enabled managedBy } webhookEvents }
query($id: ID!) { webhookDeliveries(id: $id) { event status attempts responseStatus error } }
mutation($id: ID!) { testWebhook(id: $id) { status responseStatus error } }
mutation($id: ID!) { rotateWebhookSecret(id: $id) }
mutation($id: ID!) { updateWebhook(id: $id, input: { enabled: false }) { id } }
mutation($id: ID!) { deleteWebhook(id: $id) }
```
