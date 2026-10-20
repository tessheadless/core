#!/usr/bin/env node
// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// Copies a WordPress site WITHOUT plugins to Tess: media, authors,
// categories, tags, posts and pages (models from wordpress-models.yaml).
//
//   npm install                       # turndown, to convert HTML to Markdown
//   WP_URL=https://blog.example.com WP_USER=admin WP_APP_PASSWORD="abcd efgh …" \
//   TESS_URL=https://cms.example.com TESS_TOKEN=… \
//   node wordpress.mjs
//
// WP_APP_PASSWORD is an Application Password (Users → Profile), so drafts and
// raw titles can be read. Node.js 20+. Read-only on the WordPress side.

import TurndownService from "turndown";
import { gfm } from "turndown-plugin-gfm";

const env = (name) => process.env[name]?.replace(/\/$/, "") || fail(`set ${name}`);
const WP = env("WP_URL");
const WP_AUTH = `Basic ${Buffer.from(`${env("WP_USER")}:${process.env.WP_APP_PASSWORD ?? fail("set WP_APP_PASSWORD")}`).toString("base64")}`;
const TESS = env("TESS_URL");
const TESS_TOKEN = env("TESS_TOKEN");

function fail(message) {
  console.error(message);
  process.exit(1);
}

/** Every item of a WordPress REST collection (100 per page). */
async function all(endpoint, params = "") {
  const items = [];
  for (let page = 1; ; page++) {
    const res = await fetch(`${WP}/wp-json/wp/v2/${endpoint}?per_page=100&page=${page}${params}`, {
      headers: { Authorization: WP_AUTH },
    });
    if (!res.ok) fail(`WordPress ${endpoint}: HTTP ${res.status} ${await res.text()}`);
    items.push(...(await res.json()));
    if (page >= Number(res.headers.get("x-wp-totalpages") ?? 1)) return items;
  }
}

async function tess(query, variables = {}) {
  const res = await fetch(`${TESS}/graphql`, {
    method: "POST",
    headers: { Authorization: `Bearer ${TESS_TOKEN}`, "Content-Type": "application/json" },
    body: JSON.stringify({ query, variables }),
  });
  const body = await res.json();
  if (body.errors?.length) throw new Error(body.errors.map((e) => e.message).join("; "));
  return body.data;
}

// ---- HTML → Markdown ------------------------------------------------------------
const turndown = new TurndownService({ headingStyle: "atx", codeBlockStyle: "fenced", bulletListMarker: "-" });
turndown.use(gfm); // tables, strikethrough
turndown.remove(["script", "style", "noscript"]);
// Embeds (YouTube, maps…), video and audio become links.
turndown.addRule("embeds", {
  filter: ["iframe", "video", "audio"],
  replacement: (_, node) => {
    const src = node.getAttribute("src") || node.querySelector?.("source")?.getAttribute("src");
    return src ? `\n\n[${node.getAttribute("title") || "Embedded media"}](${src})\n\n` : "";
  },
});
// Tess refuses raw HTML in Markdown: a literal "<" in the text stays text.
const escape = turndown.escape.bind(turndown);
turndown.escape = (text) => escape(text).replace(/</g, "\\<");

const ENTITIES = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " ", hellip: "…", ndash: "–", mdash: "—" };
/** Plain text from a rendered WordPress string (term names, user names). */
const text = (html) =>
  (html ?? "")
    .replace(/<[^>]*>/g, "")
    .replace(/&#(x?)([0-9a-f]+);/gi, (_, x, n) => String.fromCodePoint(parseInt(n, x ? 16 : 10)))
    .replace(/&(\w+);/g, (m, name) => ENTITIES[name] ?? m)
    .trim();
const slug = (s) => decodeURIComponent(s);

// ---- 1. Media: WordPress id → Tess asset ----------------------------------------
const assets = new Map(); // WordPress media id → Tess asset id
const urls = new Map(); // WordPress file URL (any size) → Tess file URL
for (const m of await all("media", "&context=edit")) {
  const name = decodeURIComponent(m.source_url.split("/").pop());
  const bytes = await (await fetch(m.source_url)).arrayBuffer();
  const res = await fetch(`${TESS}/media/upload?fileName=${encodeURIComponent(name)}`, {
    method: "POST",
    headers: { Authorization: `Bearer ${TESS_TOKEN}`, "Content-Type": m.mime_type },
    body: bytes,
  });
  if (!res.ok) fail(`upload ${name}: HTTP ${res.status} ${await res.text()}`);
  const asset = await res.json();
  const meta = { alt: m.alt_text || null, width: m.media_details?.width, height: m.media_details?.height };
  await tess(`mutation($id: ID!, $data: AssetUpdateInput!) { updateAsset(id: $id, data: $data) { id } }`, {
    id: asset.id,
    data: Object.fromEntries(Object.entries(meta).filter(([, v]) => v != null)),
  });
  await tess(`mutation($id: ID!) { publishAsset(id: $id) { id } }`, { id: asset.id });
  assets.set(m.id, asset.id);
  // Relative URLs (files served by Tess) become absolute for the content.
  const url = asset.url.startsWith("/") ? `${TESS}${asset.url}` : asset.url;
  for (const size of [m, ...Object.values(m.media_details?.sizes ?? {})]) urls.set(size.source_url, url);
  console.log(`media     ${name}`);
}

/** Post HTML → Markdown, with images pointing to their copies in Tess. */
function markdown(html) {
  let md = turndown.turndown(html ?? "");
  for (const [from, to] of [...urls].sort((a, b) => b[0].length - a[0].length)) md = md.replaceAll(from, to);
  return md;
}

// ---- 2. Entries without relations ------------------------------------------------
// Each model: endpoint, Tess name, WordPress item → Tess data, relations (Tess field → target ids).
const MODELS = [
  {
    endpoint: "users",
    params: "&context=edit",
    tess: "Author",
    data: (u) => ({ name: text(u.name), slug: slug(u.slug), bio: u.description || null }),
  },
  {
    endpoint: "categories",
    tess: "Category",
    data: (c) => ({ name: text(c.name), slug: slug(c.slug), description: c.description || null }),
    relations: (c) => ({ parent: c.parent ? ["categories", c.parent] : null }),
  },
  { endpoint: "tags", tess: "Tag", data: (t) => ({ name: text(t.name), slug: slug(t.slug) }) },
  {
    endpoint: "posts",
    params: "&context=edit&status=publish,future,draft,pending,private",
    tess: "Post",
    data: (p) => ({
      title: text(p.title.raw) || "(untitled)",
      slug: slug(p.slug || p.generated_slug || String(p.id)),
      date: p.date_gmt ? `${p.date_gmt}Z` : null,
      excerpt: p.excerpt.raw || null,
      body: markdown(p.content.rendered),
      sticky: p.sticky,
    }),
    relations: (p) => ({
      author: ["users", p.author],
      featuredImage: p.featured_media ? ["media", p.featured_media] : null,
      categories: p.categories.map((id) => ["categories", id]),
      tags: p.tags.map((id) => ["tags", id]),
    }),
  },
  {
    endpoint: "pages",
    params: "&context=edit&status=publish,future,draft,pending,private",
    tess: "Page",
    data: (p) => ({
      title: text(p.title.raw) || "(untitled)",
      slug: slug(p.slug || p.generated_slug || String(p.id)),
      body: markdown(p.content.rendered),
      menuOrder: p.menu_order,
    }),
    relations: (p) => ({
      author: ["users", p.author],
      featuredImage: p.featured_media ? ["media", p.featured_media] : null,
      parent: p.parent ? ["pages", p.parent] : null,
    }),
  },
];

const created = new Map(); // `${endpoint}:${id}` → { id, published, item, model }
const failed = [];
const notes = [];
for (const model of MODELS) {
  for (const item of await all(model.endpoint, model.params ?? "")) {
    // Terms and users have no status; only `publish` is public for posts and pages.
    const published = item.status === undefined || item.status === "publish";
    try {
      const { [`create${model.tess}`]: c } = await tess(
        `mutation($data: ${model.tess}CreateInput!) { create${model.tess}(data: $data) { id } }`,
        { data: model.data(item) },
      );
      if (published) await tess(`mutation($id: ID!) { publish${model.tess}(id: $id) { id } }`, { id: c.id });
      created.set(`${model.endpoint}:${item.id}`, { id: c.id, published, item, model });
      console.log(`${model.tess.padEnd(9)} ${item.id}${published ? "" : ` (${item.status} → draft)`}`);
      if (["private", "future", "pending"].includes(item.status)) notes.push(`${model.endpoint}/${item.id} was ${item.status}`);
    } catch (e) {
      failed.push(`${model.endpoint}/${item.id}: ${e.message}`);
    }
  }
}

// ---- 3. Relations ---------------------------------------------------------------
const idOf = ([target, id]) => (target === "media" ? assets.get(id) : created.get(`${target}:${id}`)?.id);
for (const [key, { id, published, item, model }] of created) {
  if (!model.relations) continue;
  const data = {};
  for (const [field, ref] of Object.entries(model.relations(item))) {
    if (ref === null) continue;
    // A single relation is [target, id]; a list is [[target, id], …], maybe empty.
    data[field] = typeof ref[0] === "string" ? (idOf(ref) ?? null) : ref.map(idOf).filter(Boolean);
  }
  try {
    await tess(`mutation($id: ID!, $data: ${model.tess}UpdateInput!) { update${model.tess}(id: $id, data: $data) { id } }`, {
      id,
      data,
    });
    if (published) await tess(`mutation($id: ID!) { publish${model.tess}(id: $id) { id } }`, { id });
  } catch (e) {
    failed.push(`${key} (relations): ${e.message}`);
  }
}

console.log(`\nDone: ${assets.size} files, ${created.size} entries.`);
if (notes.length) console.log("Copied as drafts (Tess has no private or scheduled entries):", notes.join(", "));
if (failed.length) {
  console.log(`\n${failed.length} failed:\n  ${failed.join("\n  ")}`);
  process.exitCode = 1;
}
