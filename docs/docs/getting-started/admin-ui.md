---
sidebar_position: 3
title: Admin UI
---

# Admin UI

Tess ships with an admin interface at `/admin/`. It's a static single-page
app served by the Tess binary, and it talks to the same GraphQL API as your
applications.

## Signing in

- **First run:** when no account exists yet, the admin asks for the bootstrap
  token and the first Owner's details, then signs you in.
- **Afterwards:** sign in with your email and password.
- **Tokens:** API tokens and the bootstrap token can also be used, via
  **Use an API/bootstrap token instead**.
- **Where the session lives:** in the browser tab's session storage, never in
  local storage. Closing the tab signs you out.

## Pages

Which pages you see depends on your role:

| Page | Who | What it does |
|---|---|---|
| **Content** | Everyone signed in | The **content manager**: pick a content type, search and page through its entries (drafts included), and open one in a side panel. Every field type has its own control: text with length counter, Markdown (toolbar, image upload and a live preview side by side; raw HTML is flagged as you type), numbers, dates, UUID, password (write-only: leave empty to keep it), JSON, enums, relation pickers (with **Upload…** for `Asset` relations), components and dynamic zones (add, reorder, remove blocks). What you can do follows your role, see below. |
| **Content models** | Owner, Admin | **Create**, **edit** and **delete** content types: API IDs, fields (every type with its options: length bounds and format presets for text, enum values, relation targets, components, dynamic zones…), descriptions, and the **review workflow** toggle. The built-in `Asset` type (media library) is read-only; reference it from your models with a relation field. |
| **Components** | Owner, Admin | Create, edit and delete reusable groups of fields, and see which models use each one. See [Components](../developers/components.md). |
| **Review queue** | Owner, Admin, Reviewer (and Contributors, read-only) | Entries **in review** and with **changes requested**. A side panel compares the draft with the published version and shows the history; reviewers approve (optionally with a comment) or request changes (comment required). |
| **Users** | Owner, Admin | Create, edit, disable and delete users; set roles and scope. Only Owners manage Owners, and there is always at least one Owner. |
| **API tokens** | Owner, Admin | Create tokens with a role, scope and expiry (the secret is shown once), list and revoke them. |
| **Plugins** | Owner, Admin | Upload WebAssembly modules, enable them, scope them to content types, configure them, allow outgoing hosts, read their last error. See [Managing plugins](../plugins/managing.md). |
| **My account** | Everyone signed in | Change your password; sign out of every session. |

### Content manager permissions

The buttons shown mirror the server rules; the server has the last word.

| Role | Create / edit drafts | Publish, unpublish | Submit for review | Delete |
|---|---|---|---|---|
| Owner, Admin | Every type | Every type | Workflow types | Any entry |
| Contributor | Types in their scope | In scope, types **without** the review workflow | In scope, workflow types | In scope, entries never published |
| Reviewer, Member | — (read only) | — | — | — |

Reviewers approve or request changes from the **Review queue**. API tokens
signed in to the admin can edit only if they have an admin role.

## Serving the admin

- **Docker image:** includes the admin under `/admin/`.
- **Running from source:** build it with `cd admin && npm install && npm run
  build`, then point `TESS_ADMIN_DIR` at `admin/dist/client`. Tess detects
  that folder automatically when started from the repository root.
- **Development:** `npm run dev` proxies `/graphql`, `/api` and `/media` to
  `TESS_API_URL` (default `http://localhost:8080`).
