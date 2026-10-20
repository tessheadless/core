#!/usr/bin/env node
// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// Loads content/seed.json into Tess and publishes it.
//
//   TESS_API_URL=https://cms.example.com TESS_TOKEN=… npm run seed [-- --overwrite]
//
// Import the models first: tess-transfer import content/models.yaml
// Entries are matched by slug (pages, competitors), title (features), or the
// single SiteSettings entry. Existing entries are left alone unless you pass
// --overwrite, so edits made in the admin are never lost by accident.

import { readFile } from "node:fs/promises";

const url = process.env.TESS_API_URL?.replace(/\/$/, "");
const token = process.env.TESS_TOKEN?.trim();
const overwrite = process.argv.includes("--overwrite");
if (!url || !token) {
  console.error("Set TESS_API_URL and TESS_TOKEN (a Contributor or Admin token).");
  process.exit(1);
}

const seed = JSON.parse(await readFile(new URL("../content/seed.json", import.meta.url), "utf8"));

async function gql(query, variables = {}) {
  const res = await fetch(`${url}/graphql`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
    body: JSON.stringify({ query, variables }),
  });
  const body = await res.json().catch(() => null);
  if (body?.errors?.length) throw new Error(body.errors.map((e) => e.message).join("; "));
  if (!res.ok || !body?.data) throw new Error(`HTTP ${res.status}`);
  return body.data;
}

/** GraphQL output shape → input shape: drops nulls, zone items become `{ Component: {...} }`. */
function toInput(value) {
  if (Array.isArray(value)) return value.map(toInput);
  if (value === null || typeof value !== "object") return value;
  const { __typename, ...rest } = value;
  const fields = Object.fromEntries(
    Object.entries(rest)
      .filter(([, v]) => v !== null && v !== undefined)
      .map(([k, v]) => [k, toInput(v)]),
  );
  return __typename ? { [__typename]: fields } : fields;
}

/**
 * Creates or (with --overwrite) updates an entry, then publishes it.
 * `find` returns the draft id of the existing entry, if any.
 */
async function upsert(type, label, data, find) {
  const existing = await find();
  let id = existing;
  if (existing && !overwrite) {
    console.log(`  = ${type} ${label} (exists, kept)`);
    return;
  }
  if (existing) {
    await gql(`mutation($id: ID!, $data: ${type}UpdateInput!) { update${type}(id: $id, data: $data) { id } }`, {
      id: existing,
      data,
    });
    console.log(`  ~ ${type} ${label} (updated)`);
  } else {
    const r = await gql(`mutation($data: ${type}CreateInput!) { create${type}(data: $data) { id } }`, { data });
    id = r[`create${type}`].id;
    console.log(`  + ${type} ${label}`);
  }
  await gql(`mutation($id: ID!) { publish${type}(id: $id) { id } }`, { id });
}

function finder(list, field, value) {
  return async () => {
    const data = await gql(
      `query($v: String!) { ${list}(where: { ${field}: $v }, stage: DRAFT, first: 1) { items { id } } }`,
      { v: value },
    );
    return data[list].items[0]?.id;
  };
}

try {
  await gql("{ siteSettingsEntries(first: 1) { total } }");
} catch (e) {
  console.error(`The Site* models are missing (${e.message}).\nRun: tess-transfer import content/models.yaml`);
  process.exit(1);
}

console.log(`Seeding ${url}`);
await upsert("SiteSettings", "(site settings)", toInput(seed.settings), async () => {
  const d = await gql("{ siteSettingsEntries(first: 1, stage: DRAFT) { items { id } } }");
  return d.siteSettingsEntries.items[0]?.id;
});
for (const f of seed.features) {
  await upsert("SiteFeature", f.title, toInput(f), finder("siteFeatures", "title", f.title));
}
for (const p of seed.pages) {
  await upsert("SitePage", p.slug, toInput(p), finder("sitePages", "slug", p.slug));
}
for (const c of seed.competitors) {
  await upsert("SiteCompetitor", c.slug, toInput(c), finder("siteCompetitors", "slug", c.slug));
}
console.log("Done. Rebuild the website to see the content.");
