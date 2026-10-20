#!/usr/bin/env node
// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

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
