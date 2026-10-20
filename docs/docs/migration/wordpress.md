---
sidebar_position: 3
title: From WordPress
description: Why moving a WordPress site is mostly about its plugins, and a complete migration of a site without plugins.
---

# Migrate from WordPress

Moving the **content** of a WordPress site to Tess is easy. Moving a
**real-world WordPress site** rarely is, because of its **plugins**. This page
explains why, how to plan around it, and then migrates a site **without
plugins**, step by step, with a script.

## Why plugins make it hard

A WordPress site is WordPress plus the plugins and theme installed on it.
Most of what makes a site unique lives in those plugins, and each one stores
its data in its own way:

- **Data hidden in `postmeta` and custom tables.** Advanced Custom Fields,
  Meta Box, Pods, Yoast SEO, Rank Math and most others save their values as
  serialized PHP arrays in `wp_postmeta`, or in tables of their own. None of
  it shows in the REST API unless the plugin (or your code) exposes it, and
  each plugin uses its own format.
- **Content that only makes sense with the plugin running.** Page builders
  (Elementor, Divi, WPBakery, Beaver Builder) store layouts as shortcodes or
  JSON. Without the plugin, a post is either a wall of `[vc_row][vc_column]…`
  or HTML full of layout markup, not content. Shortcodes from any plugin
  (`[gallery]`, `[contact-form-7 id="12"]`, `[product id="3"]`) turn into
  plain text the moment the plugin is gone.
- **Features, not content.** WooCommerce (products, orders, customers,
  payments), forms (Contact Form 7, Gravity Forms), memberships, comments,
  search or caching plugins provide **behavior**. A headless CMS stores content.
  These features need another service, or code in your front-end, or a
  [Tess plugin](../plugins/overview.md).
- **Multilingual plugins** (WPML, Polylang) link translations through their
  own tables and conventions.
- **Theme-dependent content.** Menus, widgets, Customizer settings and
  block patterns belong to the theme and its PHP templates.

This is why no generic tool can migrate an arbitrary WordPress site: every
plugin needs a decision, and often custom code.

### Plan the migration plugin by plugin

1. **Inventory** the active plugins (`wp plugin list --status=active` with
   [WP-CLI](https://wp-cli.org)) and the theme features you use.
2. **Classify** each one:

   | Kind | Examples | In Tess |
   |---|---|---|
   | Content model | ACF, Custom Post Type UI, Pods, Meta Box | Content types, fields, components: model them, then export the data |
   | SEO data | Yoast, Rank Math | A `Seo` component on your models |
   | Layout / page builder | Elementor, Divi, WPBakery | Rebuild pages with [dynamic zones](../developers/components.md) |
   | Behavior | WooCommerce, forms, memberships, comments | A dedicated service, your front-end, or a Tess plugin |
   | Multilingual | WPML, Polylang | One model per language, or a `locale` field |
   | Operations | caching, security, backups | Not needed: handled by Tess and your hosting |

3. **Expose** the content-model data in the REST API, if it isn't already
   there (ACF: "Show in REST API" per field group; custom post types:
   `show_in_rest`). The script below can then read it like core fields.
4. **Migrate a copy** of the site first, review it with the editors, and
   repeat until it's right: WordPress is only read.

For a site with many plugins, budget most of the effort for step 2, and for
the front-end that replaces the theme.

## A site without plugins

The rest of this page migrates a WordPress site that only uses core features:
**posts, pages, categories, tags, authors and the media library**, with the
block editor (Gutenberg) or the classic editor.

| WordPress | Tess |
|---|---|
| Post | `Post` content type |
| Page (with parent and order) | `Page` content type, `parent` relation |
| Category (hierarchical) / Tag | `Category` (with `parent`) / `Tag` |
| User who writes | `Author` (name, slug, bio: no email, no password) |
| Media library | Assets (files with alt text and size) |
| Featured image | `featuredImage` relation to `Asset` |
| Post content (HTML) | `body` (`MARKDOWN`), converted by the script |
| Published | Published |
| Draft, pending review, scheduled, private | Draft (Tess has no scheduled or private entries) |

Not copied: **comments** (use a comment service), **revisions**, **menus and
widgets** (your front-end builds navigation), and **site users' accounts**
(create Tess users for your editors: [Import & export](../devops/import-export.md)).

### 1. Import the models

```yaml title="models.yaml"
version: 1
contentTypes:
  - apiId: Author                    # WordPress users who write (no email, no password)
    fields:
      - { name: name, type: TEXT, required: true }
      - { name: slug, type: TEXT, required: true, maxLength: 200 }
      - { name: bio, type: TEXT, maxLength: null }
  - apiId: Category
    pluralApiId: categories
    fields:
      - { name: name, type: TEXT, required: true, maxLength: 200 }
      - { name: slug, type: TEXT, required: true, maxLength: 200 }
      - { name: description, type: TEXT, maxLength: null }
      - { name: parent, type: RELATION, relationTarget: Category }
  - apiId: Tag
    fields:
      - { name: name, type: TEXT, required: true, maxLength: 200 }
      - { name: slug, type: TEXT, required: true, maxLength: 200 }
  - apiId: Post
    fields:
      - { name: title, type: TEXT, required: true, maxLength: 300 }
      - { name: slug, type: TEXT, required: true, maxLength: 200 }
      - { name: date, type: DATETIME }                              # original publication date
      - { name: excerpt, type: TEXT, maxLength: null }
      - { name: body, type: MARKDOWN }                              # converted from HTML
      - { name: sticky, type: BOOLEAN }
      - { name: featuredImage, type: RELATION, relationTarget: Asset }
      - { name: author, type: RELATION, relationTarget: Author }
      - { name: categories, type: RELATION, relationTarget: Category, many: true }
      - { name: tags, type: RELATION, relationTarget: Tag, many: true }
  - apiId: Page
    fields:
      - { name: title, type: TEXT, required: true, maxLength: 300 }
      - { name: slug, type: TEXT, required: true, maxLength: 200 }
      - { name: body, type: MARKDOWN }
      - { name: menuOrder, type: INT }
      - { name: featuredImage, type: RELATION, relationTarget: Asset }
      - { name: author, type: RELATION, relationTarget: Author }
      - { name: parent, type: RELATION, relationTarget: Page }
```

```bash
export TESS_URL=https://cms.example.com
export TESS_TOKEN=…                          # Admin token
tess-transfer import models.yaml --dry-run
tess-transfer import models.yaml
```

### 2. Copy the content

In WordPress, create an **Application Password** for an administrator
(**Users → Profile → Application Passwords**, WordPress 5.6+, over HTTPS). The
script reads the REST API (`/wp-json/wp/v2/…`) with it, so drafts and raw
titles are included. Then:

```bash
npm install turndown@7.2.4 turndown-plugin-gfm@1.0.2    # HTML → Markdown
export WP_URL=https://blog.example.com WP_USER=admin WP_APP_PASSWORD="abcd efgh ijkl mnop qrst uvwx"
export TESS_URL=https://cms.example.com TESS_TOKEN=…
node wordpress.mjs
```

The script needs Node.js 20 or later. It:

1. **Uploads the media library** to Tess, with alt texts and sizes, and
   publishes each file.
2. **Creates authors, categories, tags, posts and pages**: published content
   stays published, everything else becomes a draft (it lists the private and
   scheduled ones).
3. **Converts post HTML to Markdown**: paragraphs, headings, emphasis, links,
   lists, quotes, tables and code blocks. Images point to their copy in Tess
   (resized variants included). Embeds (YouTube, maps…), video and audio
   become links. Block comments (`<!-- wp:… -->`) disappear, and a literal
   `<` in the text is escaped: Tess accepts no raw HTML in Markdown.
4. **Links relations** in a second pass: authors, featured images,
   categories, tags, parent categories and parent pages.
5. **Reports failures** without stopping, and exits with an error code if
   there were any.

{/* include: examples/migrate/wordpress.mjs */}

```js title="wordpress.mjs"
#!/usr/bin/env node
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
```

It's also in the repository, with its `package.json`:
[`examples/migrate/wordpress.mjs`](https://github.com/tessheadless/core/blob/main/examples/migrate/wordpress.mjs).

:::tip Image URLs in the content
Inline images use the file URLs Tess returns. With files served by Tess or a
CDN (`TESS_MEDIA_PUBLIC_URL`), they are stable. With a private bucket, they
are presigned and expire: set `TESS_MEDIA_PUBLIC_URL` before migrating. See
[Media storage](../devops/media-storage.md).
:::

### 3. Replace the theme

Tess is headless: your front-end renders the pages the theme used to render.
Query what a post template needs:

```graphql
query Post($slug: String!) {
  posts(where: { slug: $slug }, first: 1) {
    items {
      ref title date excerpt body
      featuredImage { url alt width height }
      author { name slug }
      categories { name slug }
      tags { name slug }
    }
  }
}
```

Render `body` with a Markdown library, and keep your URLs: WordPress
permalinks (`/2026/01/my-post/`, `/category/news/`) map to the `slug` fields.
Add redirects for anything that changes. Starters for several frameworks are
in [Clients](../developers/clients.md).

### Checklist

- Counts match: posts, pages, categories, tags and media.
- A few long posts render the same, including images, tables and embeds.
- Search the Markdown for leftover shortcodes (`[gallery`, `[caption`…) and
  rewrite them by hand.
- Old permalinks redirect to the new pages.
- Editors have Tess accounts with the right roles.
