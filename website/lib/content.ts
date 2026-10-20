// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// Website content, read from Tess at build time (prerendering), never in the browser.
//
//   TESS_API_URL=https://cms.example.com   Tess instance holding the `Site*` models
//   TESS_TOKEN=…                           optional; only needed when public reads are disabled
//
// Without TESS_API_URL the site is built from content/seed.json (the same
// content `npm run seed` loads into Tess), so it also builds offline.

import seed from "../content/seed.json";

export interface SiteLink {
  label: string;
  url: string;
}

export interface Settings {
  siteName: string;
  tagline: string;
  description: string;
  navigation: SiteLink[];
  cta: SiteLink | null;
  footerLinks: SiteLink[];
  footerText: string | null;
  docsUrl: string;
}

export type FeatureIcon =
  | "BOLT"
  | "GRAPH"
  | "DATABASE"
  | "LOCK"
  | "PLUG"
  | "CLOUD"
  | "IMAGE"
  | "SEARCH"
  | "USERS"
  | "WORKFLOW"
  | "CODE"
  | "ROBOT";

export interface Feature {
  title: string;
  summary: string;
  icon: FeatureIcon | null;
  highlighted: boolean | null;
  order: number | null;
}

export type Section =
  | {
      __typename: "SiteHero";
      eyebrow?: string | null;
      title: string;
      subtitle?: string | null;
      primary?: SiteLink | null;
      secondary?: SiteLink | null;
      code?: string | null;
    }
  | { __typename: "SiteFeatureGrid"; title: string; subtitle?: string | null; highlightedOnly?: boolean | null }
  | { __typename: "SiteRichText"; title?: string | null; body: string }
  | { __typename: "SiteCodeSample"; title: string; body?: string | null; language?: string | null; code: string }
  | { __typename: "SiteComparisonTeaser"; title: string; subtitle?: string | null }
  | { __typename: "SiteFaq"; title: string; items: { question: string; answer: string }[] }
  | {
      __typename: "SiteCallToAction";
      title: string;
      body?: string | null;
      primary?: SiteLink | null;
      secondary?: SiteLink | null;
    };

export interface Page {
  slug: string;
  title: string;
  description: string;
  sections: Section[];
}

export interface ComparisonRow {
  criterion: string;
  tess: string;
  other: string;
}

export interface Competitor {
  name: string;
  slug: string;
  website: string | null;
  kind: string;
  summary: string;
  rows: ComparisonRow[];
  chooseThem: string | null;
  chooseTess: string | null;
  migration: string | null;
  importExportUrl: string;
  /** Step-by-step migration guide, when the docs have one. */
  migrationGuideUrl: string | null;
  asOf: string;
  order: number | null;
}

const LINK = "label url";

const SETTINGS_QUERY = `query WebsiteSettings {
  siteSettingsEntries(first: 1) {
    items { siteName tagline description navigation { ${LINK} } cta { ${LINK} } footerLinks { ${LINK} } footerText docsUrl }
  }
}`;

const FEATURES_QUERY = `query WebsiteFeatures {
  siteFeatures(orderBy: [order_ASC], first: 200) { items { title summary icon highlighted order } }
}`;

const SECTIONS = `sections {
  __typename
  ... on SiteHero { eyebrow title subtitle primary { ${LINK} } secondary { ${LINK} } code }
  ... on SiteFeatureGrid { title subtitle highlightedOnly }
  ... on SiteRichText { title body }
  ... on SiteCodeSample { title body language code }
  ... on SiteComparisonTeaser { title subtitle }
  ... on SiteFaq { title items { question answer } }
  ... on SiteCallToAction { title body primary { ${LINK} } secondary { ${LINK} } }
}`;

const PAGES_QUERY = `query WebsitePages {
  sitePages(first: 1000) { items { slug title description ${SECTIONS} } }
}`;

const COMPETITORS_QUERY = `query WebsiteCompetitors {
  siteCompetitors(orderBy: [order_ASC], first: 200) {
    items {
      name slug website kind summary rows { criterion tess other }
      chooseThem chooseTess migration importExportUrl migrationGuideUrl asOf order
    }
  }
}`;

interface Content {
  settings: Settings;
  features: Feature[];
  pages: Page[];
  competitors: Competitor[];
}

async function gql<T>(url: string, query: string): Promise<T> {
  const headers: Record<string, string> = { "Content-Type": "application/json", Accept: "application/json" };
  const token = process.env.TESS_TOKEN?.trim();
  if (token) headers.Authorization = `Bearer ${token}`;
  const res = await fetch(`${url.replace(/\/$/, "")}/graphql`, {
    method: "POST",
    headers,
    body: JSON.stringify({ query }),
  });
  const body = (await res.json().catch(() => null)) as { data?: T; errors?: { message: string }[] } | null;
  if (body?.errors?.length) throw new Error(`Tess: ${body.errors.map((e) => e.message).join("; ")}`);
  if (!res.ok || !body?.data) throw new Error(`Tess: HTTP ${res.status} from ${url}`);
  return body.data;
}

async function fromTess(url: string): Promise<Content> {
  const [settings, features, pages, competitors] = await Promise.all([
    gql<{ siteSettingsEntries: { items: Settings[] } }>(url, SETTINGS_QUERY),
    gql<{ siteFeatures: { items: Feature[] } }>(url, FEATURES_QUERY),
    gql<{ sitePages: { items: Page[] } }>(url, PAGES_QUERY),
    gql<{ siteCompetitors: { items: Competitor[] } }>(url, COMPETITORS_QUERY),
  ]);
  const first = settings.siteSettingsEntries.items[0];
  if (!first) throw new Error("Tess: publish one SiteSettings entry (run `npm run seed`)");
  return {
    settings: first,
    features: features.siteFeatures.items,
    pages: pages.sitePages.items,
    competitors: competitors.siteCompetitors.items,
  };
}

let cache: Promise<Content> | undefined;

/** All the website content, loaded once per build. */
export function content(): Promise<Content> {
  if (!cache) {
    const url = process.env.TESS_API_URL?.trim();
    if (url) {
      cache = fromTess(url);
    } else {
      console.warn("[website] TESS_API_URL is not set: building from content/seed.json");
      cache = Promise.resolve(seed as unknown as Content);
    }
  }
  return cache;
}

export async function page(slug: string): Promise<Page | undefined> {
  return (await content()).pages.find((p) => p.slug === slug);
}

export async function competitor(slug: string): Promise<Competitor | undefined> {
  return (await content()).competitors.find((c) => c.slug === slug);
}

/** Data every page needs: settings for the layout. */
export interface LayoutData {
  settings: Settings;
}
