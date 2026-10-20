#!/usr/bin/env node
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
