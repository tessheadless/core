---
sidebar_position: 3
title: Workflows with n8n
---

# Workflows with n8n

[n8n](https://n8n.io) automates Tess: start workflows on Tess
[events](webhooks.md#events), read and write content and translations, and
connect hundreds of other services (Slack, email, AI translation, CI…).

Tess embeds the n8n editor in the admin UI (**Workflows**, for Admins and
Contributors), served by Tess under `/n8n/` so it shares the admin's origin.

```text
browser ──▶ Tess /admin/workflows ── iframe ──▶ Tess /n8n/* ──(proxy)──▶ n8n
n8n  ── Tess node (GraphQL, API token) ──────────────────────────────▶ Tess /graphql
Tess ── webhook (signed events) ──▶ Tess Trigger node in n8n
```

:::caution License
n8n is distributed under the [Sustainable Use License](https://docs.n8n.io/sustainable-use-license/):
running it for your own internal workflows is fine, but offering it embedded in
a product to third parties requires a commercial *n8n Embed* license from n8n.

For this reason, Tess's **commercial license** (`TESS_LICENSE=commercial`) does
not include the embedded editor: Tess doesn't proxy `/n8n/`, and **Workflows**
disappears from the admin. [Webhooks](webhooks.md) still send every event, to
n8n or any other tool you run under your own license.
:::

## Setup with Docker Compose

1. In `.env`:

   ```sh
   TESS_N8N_URL=http://n8n:5678
   N8N_ENCRYPTION_KEY=...            # openssl rand -hex 32
   N8N_PUBLIC_URL=http://localhost:8080/n8n/
   # behind HTTPS: N8N_PUBLIC_URL=https://cms.example.com/n8n/ and N8N_SECURE_COOKIE=true
   ```

2. Start n8n (the image built from `deploy/n8n/Dockerfile` includes the Tess
   nodes) and restart Tess:

   ```sh
   docker compose --profile n8n up -d --build
   ```

3. Open **Admin → Workflows** and create the n8n owner account (n8n has its
   own users; the community edition has no single sign-on with Tess).

4. In n8n, create a **Tess API** credential: **Tess URL** `http://tess:8080`
   (as n8n reaches Tess) and an [API token](../developers/authentication.md)
   with the Admin role (the trigger registers webhooks, which is an Admin
   operation).

Without Compose, run n8n anywhere with `N8N_PATH=/n8n/`,
`N8N_EDITOR_BASE_URL` and `WEBHOOK_URL` set to the public `/n8n/` URL of Tess,
`N8N_PUSH_BACKEND=sse`, and `N8N_PROXY_HOPS=1`; install the nodes from
`integrations/n8n-nodes-tess` (`npm ci && npm run build`, then point
`N8N_CUSTOM_EXTENSIONS` at the folder); set `TESS_N8N_URL` on Tess.

## Nodes

**Tess Trigger** starts the workflow on the selected events (`entry.publish`,
`entry.*`, `contentType.update`, `i18n.translation.update`…), optionally for
some content types. Activating the workflow registers a webhook in Tess
(listed under **Admin → Webhooks**, managed by `n8n`); deactivating removes
it. Signatures are verified. If Tess reaches n8n at another address than the
public one, set **n8n URL for Tess** (e.g. `http://n8n:5678`).

**Tess** reads and writes through the GraphQL API:

| Resource | Operations |
|---|---|
| Entry | Get, Get Many (filter, stage, limit), Create, Update, Delete, Publish, Unpublish |
| Translation | Get Keys (search, missing in a language), Set Text (text and/or review state), Export, Publish to delivery targets |
| Calendar | Create Event (with invitations), Update Event, Cancel Event, Get Events, Get Bookings ([calendars](calendars.md)) |
| Form | Get Submissions ([forms](forms.md)) |
| GraphQL | Any query or mutation, with variables |

After a Tess Trigger, `{{ $json.data.entry.id }}` and `{{ $json.contentType }}`
point at the changed entry.

## Ideas

- `entry.publish` → rebuild a static site (GitHub `repository_dispatch`, Netlify, Vercel hook).
- `i18n.key.create` → machine-translate the new key, save it as `TRANSLATED` for review.
- `entry.submit` → notify reviewers on Slack or by email.
- `contentType.update` → regenerate client types in a repository.
- `form.submitted` (a reservation form) → create the event in a Google calendar
  with **Tess → Calendar → Create Event**, which emails the invitation.
- `calendar.booking.created` (cal.diy) → add the guest to a mailing list, or
  post the booking on Slack.

## Notes

- The editor's live updates use server-sent events, which the Tess proxy
  streams; WebSockets are not proxied (hence `N8N_PUSH_BACKEND=sse`).
- Responses written by workflows (`/n8n/webhook/*`, `/n8n/form/*`) are
  sandboxed with a `Content-Security-Policy` so they cannot run scripts with
  the Tess admin's origin.
- The proxy is not available on AWS Lambda, nor behind the
  [Passenger](../devops/passenger.md) launcher (it buffers responses): link to
  n8n on its own domain there.
