#!/usr/bin/env node
// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// Loads content/seed.json (English) and content/seed.{fr,zh,es}.json into
// Tess and publishes them, each entry with its `locale`.
//
//   TESS_API_URL=https://cms.example.com TESS_TOKEN=… npm run seed [-- --overwrite] [-- --locale fr]
//
// Import the models first: tess-transfer import content/models.yaml
// Entries are matched by locale and slug (pages, competitors), title
// (features), or the SiteSettings entry of the language; English also matches
// entries without a locale. Existing entries are left alone unless you pass
// --overwrite, so edits made in the admin are never lost by accident.

import { access, readFile } from "node:fs/promises";

const url = process.env.TESS_API_URL?.replace(/\/$/, "");
const token = process.env.TESS_TOKEN?.trim();
const overwrite = process.argv.includes("--overwrite");
const only = process.argv.includes("--locale") ? process.argv[process.argv.indexOf("--locale") + 1] : undefined;
const LOCALES = ["en", "fr", "zh", "es"];
if (only && !LOCALES.includes(only)) {
  console.error(`--locale must be one of ${LOCALES.join(", ")}`);
  process.exit(1);
}
if (!url || !token) {
  console.error("Set TESS_API_URL and TESS_TOKEN (a Contributor or Admin token).");
  process.exit(1);
}

/** The seed of a language, or null when its file doesn't exist. */
async function readSeed(locale) {
  const file = new URL(`../content/seed${locale === "en" ? "" : `.${locale}`}.json`, import.meta.url);
  try {
    await access(file);
  } catch {
    return null;
  }
  return JSON.parse(await readFile(file, "utf8"));
}

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

/** Does an entry belong to `locale`? Entries without a locale are English. */
const sameLocale = (entry, locale) => (entry.locale ?? "EN") === locale;

function finder(list, field, value, locale) {
  return async () => {
    const data = await gql(
      `query($v: String!) { ${list}(where: { ${field}: $v }, stage: DRAFT, first: 50) { items { id locale } } }`,
      { v: value },
    );
    return data[list].items.find((e) => sameLocale(e, locale))?.id;
  };
}

try {
  await gql("{ siteSettingsEntries(first: 1) { total } }");
} catch (e) {
  console.error(`The Site* models are missing (${e.message}).\nRun: tess-transfer import content/models.yaml`);
  process.exit(1);
}

console.log(`Seeding ${url}`);
for (const lang of only ? [only] : LOCALES) {
  const seed = await readSeed(lang);
  if (!seed) continue;
  const locale = lang.toUpperCase();
  const data = (entry) => toInput({ ...entry, locale });
  console.log(`[${lang}]`);
  if (seed.settings) {
    await upsert("SiteSettings", `(site settings, ${lang})`, data(seed.settings), async () => {
      const d = await gql("{ siteSettingsEntries(first: 50, stage: DRAFT) { items { id locale } } }");
      return d.siteSettingsEntries.items.find((e) => sameLocale(e, locale))?.id;
    });
  }
  for (const f of seed.features ?? []) {
    await upsert("SiteFeature", f.title, data(f), finder("siteFeatures", "title", f.title, locale));
  }
  for (const p of seed.pages ?? []) {
    await upsert("SitePage", p.slug, data(p), finder("sitePages", "slug", p.slug, locale));
  }
  for (const c of seed.competitors ?? []) {
    await upsert("SiteCompetitor", c.slug, data(c), finder("siteCompetitors", "slug", c.slug, locale));
  }
}
console.log("Done. Rebuild the website to see the content.");
