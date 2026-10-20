---
sidebar_position: 1
title: From Strapi
description: Move a Strapi 5 project to Tess, with its content types, components, media and entries.
---

# Migrate from Strapi

This guide moves a Strapi 5 project to Tess in four steps:

1. Describe your content types and components in a Tess **models file**.
2. **Import the models** with `tess-transfer`.
3. **Copy media and entries** with a script that reads the Strapi REST API
   and writes to the Tess GraphQL API.
4. **Switch your front-end** to the Tess GraphQL API.

Strapi is only read, so you can rehearse the migration as often as you like.
Start with a fresh Tess instance (see [Quick start](../getting-started/quick-start.md))
and an Admin token for it.

## 1. Map your models

Strapi types map closely to Tess:

| Strapi | Tess | Notes |
|---|---|---|
| `string`, `text` | `TEXT` | Tess caps `TEXT` at 80 characters unless you set `maxLength` (`null` = unlimited). |
| `email` | `TEXT` | With a `pattern`. |
| `uid` | `TEXT` | With a `pattern` such as `^[a-z0-9-]+$`. Tess does not enforce uniqueness. |
| `richtext` (Markdown) | `MARKDOWN` | Tess rejects raw HTML in Markdown: convert it first, or use `TEXT`. |
| `blocks` (rich text) | `JSON` | Keeps Strapi's block tree as is. |
| `integer`, `biginteger` | `INT` | `biginteger` values arrive as strings: convert them. |
| `decimal`, `float` | `FLOAT` | |
| `boolean` | `BOOLEAN` | |
| `date` / `datetime` / `time` | `DATE` / `DATETIME` / `TEXT` | |
| `json` | `JSON` | |
| `enumeration` | `ENUM` | Values may only use letters, digits and `_`: `in-progress` becomes `IN_PROGRESS`. |
| `password` | `PASSWORD` | Hashes can't be copied; users set a new password. |
| `media` | `RELATION` to `Asset` | `many: true` for multiple media. |
| `relation` | `RELATION` | `many: true` for `oneToMany` and `manyToMany`. |
| `component` | `COMPONENT` | `repeatable: true` for repeatable components. |
| `dynamiczone` | `DYNAMIC_ZONE` | |

Also:

- **Names:** Tess field names are camelCase (`coverImage`), so rename
  `snake_case` attributes (`cover_image`). Model names are PascalCase.
- **Single types** become a content type with one entry.
- **Draft & Publish** maps to Tess stages: published entries stay published,
  draft-only entries stay drafts.
- **Internationalization:** Tess has no per-entry locales yet. Copy one
  locale per run (add `&locale=fr` to the Strapi requests), into one model per
  language or a model with a `locale` `ENUM` field. For UI strings, use
  [Translations](../integrations/translations.md).
- **Users:** Strapi admins become Tess users (the `users` part of the models
  file). End users of the Users & Permissions plugin are not Tess users.

### Example

A Strapi article, `src/api/article/content-types/article/schema.json`:

```json
{
  "kind": "collectionType",
  "info": { "singularName": "article", "pluralName": "articles" },
  "options": { "draftAndPublish": true },
  "attributes": {
    "title": { "type": "string", "required": true },
    "slug": { "type": "uid", "targetField": "title" },
    "content": { "type": "richtext" },
    "cover_image": { "type": "media", "multiple": false, "allowedTypes": ["images"] },
    "category": { "type": "relation", "relation": "manyToOne", "target": "api::category.category" },
    "seo": { "type": "component", "component": "shared.seo" },
    "blocks": { "type": "dynamiczone", "components": ["shared.quote", "shared.rich-text"] }
  }
}
```

becomes, in `models.yaml` (with its `Category` and components):

```yaml title="models.yaml"
version: 1
components:
  - apiId: Seo                       # Strapi component shared.seo
    fields:
      - { name: metaTitle, type: TEXT, maxLength: 60 }
      - { name: metaDescription, type: TEXT, maxLength: 160 }
  - apiId: Quote                     # shared.quote
    fields:
      - { name: title, type: TEXT }
      - { name: body, type: TEXT, maxLength: null }
  - apiId: RichText                  # shared.rich-text
    fields:
      - { name: body, type: MARKDOWN }
contentTypes:
  - apiId: Category
    pluralApiId: categories
    fields:
      - { name: name, type: TEXT, required: true }
      - { name: slug, type: TEXT, required: true, pattern: "^[a-z0-9-]+$" }
  - apiId: Article
    fields:
      - { name: title, type: TEXT, required: true, maxLength: 200 }
      - { name: slug, type: TEXT, required: true, pattern: "^[a-z0-9-]+$" }
      - { name: body, type: MARKDOWN }                              # Strapi `content` (richtext)
      - { name: cover, type: RELATION, relationTarget: Asset }      # Strapi `cover_image` (media)
      - { name: category, type: RELATION, relationTarget: Category }
      - { name: seo, type: COMPONENT, component: Seo }
      - { name: blocks, type: DYNAMIC_ZONE, components: [Quote, RichText] }
```

## 2. Import the models

```bash
export TESS_URL=https://cms.example.com
export TESS_TOKEN=…                          # Admin token
tess-transfer import models.yaml --dry-run   # check the plan
tess-transfer import models.yaml
```

See [Import & export](../devops/import-export.md) for the file format.

:::tip Required relations
The script creates entries first and links them in a second pass. Make
relation fields optional during the migration, then set `required: true` and
run `tess-transfer import models.yaml --update`.
:::

## 3. Copy media and entries

Create a **read-only API token** in Strapi (Settings → API Tokens), then edit
the `MODELS` list at the top of the script: one line per collection type, the
Strapi → Tess field names, the relations and the dynamic zones.

```bash
export STRAPI_URL=https://strapi.example.com STRAPI_TOKEN=…
export TESS_URL=https://cms.example.com TESS_TOKEN=…
node strapi.mjs
```

The script needs Node.js 20 or later and has no dependencies. It:

1. **Uploads every file** of the Strapi media library to Tess, with its alt
   text and size, and publishes it.
2. **Creates the entries** without their relations: published entries as
   published (the version visitors see), draft-only entries as drafts.
3. **Links relations and media** in a second pass, then republishes.
4. **Lists published entries with unpublished changes** in Strapi, so editors
   can redo those edits in Tess.

{/* include: examples/migrate/strapi.mjs */}

```js title="strapi.mjs"
#!/usr/bin/env node
// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// Copies media and entries from Strapi 5 to Tess (models must exist in Tess).
//
//   STRAPI_URL=https://strapi.example.com STRAPI_TOKEN=… \
//   TESS_URL=https://cms.example.com TESS_TOKEN=… \
//   node strapi.mjs
//
// Edit MODELS below: one line per Strapi collection type, in any order.
// Node.js 20+, no dependencies. Read-only on the Strapi side.

const MODELS = [
  // Strapi API id (plural) → Tess model; `fields`: Strapi name → Tess name (default: same).
  { strapi: "categories", tess: "Category", fields: { name: "name", slug: "slug" } },
  {
    strapi: "articles",
    tess: "Article",
    fields: { title: "title", slug: "slug", content: "body", cover_image: "cover", category: "category", seo: "seo", blocks: "blocks" },
    // Relations (to other models or to media), filled in a second pass.
    relations: { cover_image: "media", category: "categories" },
    // Dynamic zones: Strapi component uid → Tess component.
    zones: { blocks: { "shared.quote": "Quote", "shared.rich-text": "RichText" } },
    // Values that need converting, e.g. enum values ("in-progress" → IN_PROGRESS):
    // convert: { state: (v) => v && v.toUpperCase().replaceAll("-", "_") },
  },
];

const env = (name) => process.env[name]?.replace(/\/$/, "") || fail(`set ${name}`);
const STRAPI = env("STRAPI_URL");
const TESS = env("TESS_URL");
const strapiAuth = { Authorization: `Bearer ${env("STRAPI_TOKEN")}` };
const tessAuth = { Authorization: `Bearer ${env("TESS_TOKEN")}` };

function fail(message) {
  console.error(message);
  process.exit(1);
}

async function strapi(path) {
  const res = await fetch(`${STRAPI}${path}`, { headers: strapiAuth });
  if (!res.ok) fail(`Strapi ${path}: HTTP ${res.status}`);
  return res.json();
}

async function tess(query, variables = {}) {
  const res = await fetch(`${TESS}/graphql`, {
    method: "POST",
    headers: { ...tessAuth, "Content-Type": "application/json" },
    body: JSON.stringify({ query, variables }),
  });
  const body = await res.json();
  if (body.errors?.length) throw new Error(body.errors.map((e) => e.message).join("; "));
  return body.data;
}

/** Every entry of a collection, in one status (`published` or `draft`). */
async function all(collection, status) {
  const items = [];
  for (let page = 1; ; page++) {
    const q = `populate=*&status=${status}&pagination[page]=${page}&pagination[pageSize]=100`;
    const { data, meta } = await strapi(`/api/${collection}?${q}`);
    items.push(...data);
    if (page >= meta.pagination.pageCount) return items;
  }
}

// ---- 1. Media: Strapi file id → Tess asset id --------------------------------
const assets = new Map();
for (const file of await strapi("/api/upload/files")) {
  const url = file.url.startsWith("http") ? file.url : `${STRAPI}${file.url}`;
  const bytes = await (await fetch(url)).arrayBuffer();
  const res = await fetch(`${TESS}/media/upload?fileName=${encodeURIComponent(file.name)}`, {
    method: "POST",
    headers: { ...tessAuth, "Content-Type": file.mime },
    body: bytes,
  });
  if (!res.ok) fail(`upload ${file.name}: HTTP ${res.status} ${await res.text()}`);
  const asset = await res.json();
  const meta = { alt: file.alternativeText, width: file.width, height: file.height };
  await tess(`mutation($id: ID!, $data: AssetUpdateInput!) { updateAsset(id: $id, data: $data) { id } }`, {
    id: asset.id,
    data: Object.fromEntries(Object.entries(meta).filter(([, v]) => v != null)),
  });
  // Published assets show in published content.
  await tess(`mutation($id: ID!) { publishAsset(id: $id) { id } }`, { id: asset.id });
  assets.set(file.id, asset.id);
  console.log(`media  ${file.name}`);
}

/** Strapi entry → Tess input, without relations. */
function toTess(model, entry) {
  const data = {};
  for (const [from, to] of Object.entries(model.fields)) {
    const value = entry[from];
    if (value === undefined || model.relations?.[from]) continue;
    const zone = model.zones?.[from];
    if (model.convert?.[from]) data[to] = model.convert[from](value);
    else if (zone) data[to] = value?.map(({ __component, ...item }) => ({ [zone[__component]]: clean(item) }));
    else data[to] = clean(value);
  }
  return data;
}

/** Drops Strapi's internal `id` of components. */
function clean(value) {
  if (Array.isArray(value)) return value.map(clean);
  if (value === null || typeof value !== "object") return value;
  const { id, ...rest } = value;
  return Object.fromEntries(Object.entries(rest).map(([k, v]) => [k, clean(v)]));
}

// ---- 2. Entries without relations: Strapi documentId → Tess id ---------------
const created = new Map(); // `${collection}:${documentId}` → { id, published, entry }
const changed = [];
for (const model of MODELS) {
  const published = await all(model.strapi, "published");
  const drafts = await all(model.strapi, "draft");
  const live = new Set(published.map((e) => e.documentId));
  // Published entries as visitors see them; draft-only entries as drafts.
  for (const entry of [...published, ...drafts.filter((d) => !live.has(d.documentId))]) {
    const isLive = live.has(entry.documentId);
    const { [`create${model.tess}`]: c } = await tess(
      `mutation($data: ${model.tess}CreateInput!) { create${model.tess}(data: $data) { id } }`,
      { data: toTess(model, entry) },
    );
    if (isLive) await tess(`mutation($id: ID!) { publish${model.tess}(id: $id) { id } }`, { id: c.id });
    created.set(`${model.strapi}:${entry.documentId}`, { id: c.id, published: isLive, entry });
    console.log(`${model.tess.padEnd(12)} ${entry.documentId}${isLive ? " (published)" : ""}`);
  }
  const draftOf = new Map(drafts.map((d) => [d.documentId, d]));
  for (const p of published) {
    if (draftOf.get(p.documentId)?.updatedAt > p.publishedAt) changed.push(`${model.strapi}/${p.documentId}`);
  }
}

// ---- 3. Relations ---------------------------------------------------------------
const idOf = (target, value) =>
  target === "media" ? assets.get(value.id) : created.get(`${target}:${value.documentId}`)?.id;
for (const model of MODELS.filter((m) => m.relations)) {
  for (const [key, { id, published, entry }] of created) {
    if (!key.startsWith(`${model.strapi}:`)) continue;
    const data = {};
    for (const [from, target] of Object.entries(model.relations)) {
      const value = entry[from];
      if (value == null) continue;
      data[model.fields[from] ?? from] = Array.isArray(value)
        ? value.map((v) => idOf(target, v)).filter(Boolean)
        : (idOf(target, value) ?? null);
    }
    if (Object.keys(data).length === 0) continue;
    await tess(`mutation($id: ID!, $data: ${model.tess}UpdateInput!) { update${model.tess}(id: $id, data: $data) { id } }`, {
      id,
      data,
    });
    if (published) await tess(`mutation($id: ID!) { publish${model.tess}(id: $id) { id } }`, { id });
  }
}

console.log(`\nDone: ${assets.size} files, ${created.size} entries.`);
if (changed.length) {
  console.log("Unpublished changes in Strapi (copy them by hand):", changed.join(", "));
}
```

It's also in the repository: [`examples/migrate/strapi.mjs`](https://github.com/tessheadless/core/blob/main/examples/migrate/strapi.mjs).
Relations and media inside components are not linked by the script. Add them
in the second pass the same way if you use them.

## 4. Switch your front-end

Strapi REST calls become GraphQL queries:

```js
// Strapi 5
const res = await fetch(`${STRAPI}/api/articles?filters[slug][$eq]=${slug}&populate=*`);
const article = (await res.json()).data[0];

// Tess
const res = await fetch(`${TESS}/graphql`, {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({
    query: `query($slug: String!) {
      articles(where: { slug: $slug }, first: 1) {
        items { title body cover { url alt } category { name } seo { metaTitle metaDescription }
                blocks { __typename ... on Quote { title body } ... on RichText { body } } }
      }
    }`,
    variables: { slug },
  }),
});
const article = (await res.json()).data.articles.items[0];
```

Use `ref` (not `id`) as a stable key in your front-end, and fetch `cover.url`
at render time: it may be a presigned URL that expires. See
[Querying](../developers/querying.md) and [Media](../developers/media.md).

## Checklist

- Entry counts match in both CMSs (`articles { total }` in Tess).
- Published pages render the same, drafts are drafts.
- Webhooks and their consumers point to Tess
  ([Webhooks](../integrations/webhooks.md)).
- Roles: Strapi editors become Contributors, reviewers Reviewers, admins
  Admins.
