---
sidebar_position: 2
title: From Hygraph
description: Move a Hygraph project to Tess, with its models, components, assets and entries.
---

# Migrate from Hygraph

Hygraph and Tess are both GraphQL-first, so the move is mostly mechanical:

1. Describe your models and components in a Tess **models file**.
2. **Import the models** with `tess-transfer`.
3. **Copy assets and entries** with a GraphQL-to-GraphQL script.
4. **Adapt your front-end queries**: most of them only change shape slightly.

Hygraph is only read, so you can rehearse the migration as often as you like.
Start with a fresh Tess instance (see [Quick start](../getting-started/quick-start.md))
and an Admin token for it.

## 1. Map your models

| Hygraph | Tess | Notes |
|---|---|---|
| Single line text, Slug | `TEXT` | Slugs with a `pattern` such as `^[a-z0-9-]+$`. Tess caps `TEXT` at 80 characters unless you set `maxLength`. |
| Multi line text | `TEXT` | `maxLength: null` (unlimited). |
| Markdown | `MARKDOWN` | Tess rejects raw HTML in Markdown. |
| Rich Text | `MARKDOWN` | Query `{ markdown }` and convert (see the script). Or keep `{ json }` in a `JSON` field. |
| Int, Float, Boolean | `INT`, `FLOAT`, `BOOLEAN` | |
| Date, DateTime | `DATE`, `DATETIME` | |
| JSON | `JSON` | |
| Enumeration | `ENUM` | Same value rules (GraphQL names). |
| Color | `TEXT` | Query `{ hex }`. |
| Location | `COMPONENT` | A `Location` component with `latitude` and `longitude` (`FLOAT`). |
| Asset | `RELATION` to `Asset` | `many: true` for multiple assets. |
| Reference | `RELATION` | `many: true` for multiple references. A reference to several models (union) becomes one relation field per model. |
| Basic component | `COMPONENT` | `repeatable: true` for "allow multiple values". |
| Modular component | `DYNAMIC_ZONE` | The allowed components. |
| "Allow multiple values" on a scalar | `JSON` | Stores the list. |

Also:

- **Stages:** `PUBLISHED` and `DRAFT` work the same way in Tess (and the
  `stage` argument has the same name). Custom stages, such as `QA`, aren't
  copied.
- **Localization:** Tess has no per-entry locales yet. Copy one locale per run
  (add `locales: [fr]` to the Hygraph queries), into one model per language or
  a model with a `locale` `ENUM` field. For UI strings, use
  [Translations](../integrations/translations.md).
- **Remote sources and remote fields** are not content: keep calling those
  services from your front-end, or from a [plugin](../plugins/overview.md).
- **System fields** (`id`, `createdAt`, `publishedAt`, `createdBy`…) are set by
  Tess.

### Example

A Hygraph `Post` model with a title, a slug, a date, tags (single line text
with multiple values), Rich Text `content`, an `Seo` component, a `blocks`
modular component (`Hero` or `Quote`), a cover image, a category and authors
becomes, in `models.yaml`:

```yaml title="models.yaml"
version: 1
components:
  - apiId: Seo                       # basic component
    fields:
      - { name: metaTitle, type: TEXT, maxLength: 60 }
      - { name: metaDescription, type: TEXT, maxLength: 160 }
  - apiId: Hero                      # modular component (union member)
    fields:
      - { name: title, type: TEXT, required: true }
      - { name: subtitle, type: TEXT, maxLength: null }
  - apiId: Quote
    fields:
      - { name: text, type: TEXT, maxLength: null }
      - { name: author, type: TEXT }
contentTypes:
  - apiId: Category
    pluralApiId: categories
    fields:
      - { name: name, type: TEXT, required: true }
      - { name: slug, type: TEXT, required: true, pattern: "^[a-z0-9-]+$" }
  - apiId: Author
    fields:
      - { name: name, type: TEXT, required: true }
      - { name: bio, type: TEXT, maxLength: null }
  - apiId: Post
    fields:
      - { name: title, type: TEXT, required: true, maxLength: 200 }
      - { name: slug, type: TEXT, required: true, pattern: "^[a-z0-9-]+$" }
      - { name: date, type: DATE }
      - { name: tags, type: JSON }                                  # String, "allow multiple values"
      - { name: body, type: MARKDOWN }                              # Rich Text `content`
      - { name: seo, type: COMPONENT, component: Seo }
      - { name: blocks, type: DYNAMIC_ZONE, components: [Hero, Quote] }
      - { name: coverImage, type: RELATION, relationTarget: Asset }
      - { name: category, type: RELATION, relationTarget: Category }
      - { name: authors, type: RELATION, relationTarget: Author, many: true }
```

:::tip Listing a Hygraph schema
The Hygraph schema editor shows every field and its type. To list them all at
once, run an introspection query against your Content API, for example
`{ __type(name: "Post") { fields { name type { name kind ofType { name kind } } } } }`.
:::

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

## 3. Copy assets and entries

In Hygraph, create a **Permanent Auth Token** (Project settings → Access →
Permanent Auth Tokens) with read access to the `DRAFT` and `PUBLISHED`
stages, and copy the **Content API** endpoint. Then edit the `MODELS` list at
the top of the script: one line per model, the fields to copy as a GraphQL
selection, the relations, the conversions and the renamed fields.

```bash
export HYGRAPH_URL=https://<region>.cdn.hygraph.com/content/<project>/master HYGRAPH_TOKEN=…
export TESS_URL=https://cms.example.com TESS_TOKEN=…
node hygraph.mjs
```

The script needs Node.js 20 or later and has no dependencies. It:

1. **Uploads every asset** to Tess, with its dimensions, and publishes it.
2. **Creates the entries** without their relations: published documents as
   published (the version visitors see), draft-only documents as drafts.
   Modular component items (`__typename`) become dynamic zone items.
3. **Links references and assets** in a second pass, then republishes.
4. **Lists published documents with unpublished changes** in Hygraph, so
   editors can redo those edits in Tess.

{/* include: examples/migrate/hygraph.mjs */}

```js title="hygraph.mjs"
#!/usr/bin/env node
// Copies assets and entries from Hygraph to Tess (models must exist in Tess).
//
//   HYGRAPH_URL=https://<region>.cdn.hygraph.com/content/<project>/master HYGRAPH_TOKEN=… \
//   TESS_URL=https://cms.example.com TESS_TOKEN=… \
//   node hygraph.mjs
//
// Edit MODELS below: one line per Hygraph model, in any order.
// Node.js 20+, no dependencies. Read-only on the Hygraph side (the token only
// needs read access to the DRAFT and PUBLISHED stages).

const MODELS = [
  // `query`: plural query field in Hygraph; `select`: fields to copy (GraphQL selection).
  { query: "categories", tess: "Category", select: "name slug" },
  {
    query: "posts",
    tess: "Post",
    select: `title slug date tags content { markdown } seo { metaTitle metaDescription }
             blocks { __typename ... on Hero { title subtitle } ... on Quote { text author } }`,
    // Values that need converting: Rich Text → MARKDOWN.
    convert: { content: (v) => v?.markdown ?? null },
    // Relations, filled in a second pass: field → `assets` or a model's `query`.
    relations: { coverImage: "assets", category: "categories", authors: "authors" },
    // Hygraph field → Tess field, when they differ.
    rename: { content: "body" },
  },
  { query: "authors", tess: "Author", select: "name bio" },
];

const env = (name) => process.env[name]?.replace(/\/$/, "") || fail(`set ${name}`);
const HYGRAPH = env("HYGRAPH_URL");
const HYGRAPH_TOKEN = env("HYGRAPH_TOKEN");
const TESS = env("TESS_URL");
const TESS_TOKEN = env("TESS_TOKEN");

function fail(message) {
  console.error(message);
  process.exit(1);
}

async function graphql(url, token, query, variables = {}) {
  const res = await fetch(url, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
    body: JSON.stringify({ query, variables }),
  });
  const body = await res.json().catch(() => ({ errors: [{ message: `HTTP ${res.status}` }] }));
  if (body.errors?.length) throw new Error(`${url}: ${body.errors.map((e) => e.message).join("; ")}`);
  return body.data;
}
const hygraph = (q, v) => graphql(HYGRAPH, HYGRAPH_TOKEN, q, v);
const tess = (q, v) => graphql(`${TESS}/graphql`, TESS_TOKEN, q, v);

/** Every document of a model in one stage (`DRAFT` holds the latest version of all). */
async function all(query, selection, stage) {
  const items = [];
  for (let skip = 0; ; skip += 100) {
    const data = await hygraph(
      `query($skip: Int!, $stage: Stage!) { items: ${query}(first: 100, skip: $skip, stage: $stage) { ${selection} } }`,
      { skip, stage },
    );
    items.push(...data.items);
    if (data.items.length < 100) return items;
  }
}

// ---- 1. Assets: Hygraph id → Tess asset id -----------------------------------
const assets = new Map();
for (const a of await all("assets", "id url fileName mimeType width height", "DRAFT")) {
  const bytes = await (await fetch(a.url)).arrayBuffer();
  const res = await fetch(`${TESS}/media/upload?fileName=${encodeURIComponent(a.fileName)}`, {
    method: "POST",
    headers: { Authorization: `Bearer ${TESS_TOKEN}`, "Content-Type": a.mimeType ?? "application/octet-stream" },
    body: bytes,
  });
  if (!res.ok) fail(`upload ${a.fileName}: HTTP ${res.status} ${await res.text()}`);
  const { id } = await res.json();
  const meta = Object.fromEntries(Object.entries({ width: a.width, height: a.height }).filter(([, v]) => v != null));
  if (Object.keys(meta).length) {
    await tess(`mutation($id: ID!, $data: AssetUpdateInput!) { updateAsset(id: $id, data: $data) { id } }`, { id, data: meta });
  }
  // Published assets show in published content.
  await tess(`mutation($id: ID!) { publishAsset(id: $id) { id } }`, { id });
  assets.set(a.id, id);
  console.log(`asset  ${a.fileName}`);
}

/** Hygraph values → Tess input: union items become `{ Component: {...} }`. */
function toInput(value) {
  if (Array.isArray(value)) return value.map(toInput);
  if (value === null || typeof value !== "object") return value;
  const { __typename, ...rest } = value;
  const fields = Object.fromEntries(Object.entries(rest).map(([k, v]) => [k, toInput(v)]));
  return __typename ? { [__typename]: fields } : fields;
}

function toTess(model, doc) {
  const data = {};
  for (const [field, value] of Object.entries(doc)) {
    if (["id", "updatedAt", "publishedAt"].includes(field) || model.relations?.[field]) continue;
    const converted = model.convert?.[field] ? model.convert[field](value) : toInput(value);
    data[model.rename?.[field] ?? field] = converted;
  }
  return data;
}

// ---- 2. Entries without relations: Hygraph id → Tess id ----------------------
const created = new Map(); // `${query}:${id}` → { id, published, doc }
const changed = [];
for (const model of MODELS) {
  const rel = Object.keys(model.relations ?? {})
    .map((f) => `${f} { ... on Node { id } }`)
    .join(" ");
  const selection = `id updatedAt publishedAt ${model.select} ${rel}`;
  const published = await all(model.query, selection, "PUBLISHED");
  const drafts = await all(model.query, selection, "DRAFT");
  const live = new Set(published.map((d) => d.id));
  for (const doc of [...published, ...drafts.filter((d) => !live.has(d.id))]) {
    const isLive = live.has(doc.id);
    const { [`create${model.tess}`]: c } = await tess(
      `mutation($data: ${model.tess}CreateInput!) { create${model.tess}(data: $data) { id } }`,
      { data: toTess(model, doc) },
    );
    if (isLive) await tess(`mutation($id: ID!) { publish${model.tess}(id: $id) { id } }`, { id: c.id });
    created.set(`${model.query}:${doc.id}`, { id: c.id, published: isLive, doc });
    console.log(`${model.tess.padEnd(12)} ${doc.id}${isLive ? " (published)" : ""}`);
  }
  const draftOf = new Map(drafts.map((d) => [d.id, d]));
  for (const p of published) {
    if (draftOf.get(p.id)?.updatedAt > p.publishedAt) changed.push(`${model.query}/${p.id}`);
  }
}

// ---- 3. Relations ---------------------------------------------------------------
const idOf = (target, ref) => (target === "assets" ? assets.get(ref.id) : created.get(`${target}:${ref.id}`)?.id);
for (const model of MODELS.filter((m) => m.relations)) {
  for (const [key, { id, published, doc }] of created) {
    if (!key.startsWith(`${model.query}:`)) continue;
    const data = {};
    for (const [field, target] of Object.entries(model.relations)) {
      const value = doc[field];
      if (value == null) continue;
      data[model.rename?.[field] ?? field] = Array.isArray(value)
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

console.log(`\nDone: ${assets.size} assets, ${created.size} entries.`);
if (changed.length) {
  console.log("Unpublished changes in Hygraph (copy them by hand):", changed.join(", "));
}
```

It's also in the repository: [`examples/migrate/hygraph.mjs`](https://github.com/tessheadless/core/blob/main/examples/migrate/hygraph.mjs).
References and assets inside components are not linked by the script. Add them
in the second pass the same way if you use them.

## 4. Adapt your front-end

Queries keep their structure. The differences:

| Hygraph | Tess |
|---|---|
| `posts(where: { slug: $slug })` returns a list | `posts(where: { slug: $slug }, first: 1) { items { … } total }` |
| `post(where: { slug: $slug })` | `posts(where: { slug: $slug }, first: 1) { items { … } }`, or `post(id:)` |
| `content { html }` (Rich Text) | `body` (Markdown, render it with your Markdown library) |
| `coverImage { url }` | `coverImage { url alt width height }` |
| `blocks { __typename ... on Hero { title } }` | Same |
| `stage: DRAFT` with a token | Same, with a Tess token |
| `orderBy: date_DESC` | `orderBy: [date_DESC]` |

```graphql
query Post($slug: String!) {
  posts(where: { slug: $slug }, first: 1) {
    items {
      ref title date tags body
      coverImage { url alt }
      category { name slug }
      authors { name }
      seo { metaTitle metaDescription }
      blocks { __typename ... on Hero { title subtitle } ... on Quote { text author } }
    }
  }
}
```

Use `ref` (not `id`) as a stable key in your front-end, and fetch asset `url`s
at render time: they may be presigned URLs that expire. See
[Querying](../developers/querying.md) and [Media](../developers/media.md).

## Checklist

- Entry counts match in both CMSs (`posts { total }` in Tess).
- Published pages render the same, drafts are drafts.
- Webhooks and their consumers point to Tess
  ([Webhooks](../integrations/webhooks.md)).
- Roles: Hygraph editors become Contributors, reviewers Reviewers, admins
  Admins.
