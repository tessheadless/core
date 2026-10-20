// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// Website content, read from Tess at build time (prerendering), never in the browser.
//
//   TESS_API_URL=https://cms.example.com   Tess instance holding the `Site*` models
//   TESS_TOKEN=…                           optional; only needed when public reads are disabled
//
// Without TESS_API_URL the site is built from content/seed*.json (the same
// content `npm run seed` loads into Tess), so it also builds offline.
//
// Every model has a `locale` field (EN, FR, ZH, ES; empty means English). A
// language without its own entry shows the English one: a missing page,
// competitor or settings entry, or no feature at all in that language.

import seed from "../content/seed.json";
import seedEs from "../content/seed.es.json";
import seedFr from "../content/seed.fr.json";
import seedZh from "../content/seed.zh.json";
import { DEFAULT_LOCALE, LOCALES, type Locale } from "./i18n";

export interface SiteLink {
  label: string;
  url: string;
}

/** Value of the `locale` ENUM field of the Site* models. */
export type LocaleValue = "EN" | "FR" | "ZH" | "ES";

interface Localized {
  locale?: LocaleValue | null;
}

export interface Settings extends Localized {
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

export interface Feature extends Localized {
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
      /** Label of `code` (default: graphql). */
      codeLanguage?: string | null;
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

export interface Page extends Localized {
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

export interface Competitor extends Localized {
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
  siteSettingsEntries(first: 20) {
    items { locale siteName tagline description navigation { ${LINK} } cta { ${LINK} } footerLinks { ${LINK} } footerText docsUrl }
  }
}`;

const FEATURES_QUERY = `query WebsiteFeatures {
  siteFeatures(orderBy: [order_ASC], first: 200) { items { locale title summary icon highlighted order } }
}`;

const SECTIONS = `sections {
  __typename
  ... on SiteHero { eyebrow title subtitle primary { ${LINK} } secondary { ${LINK} } code codeLanguage }
  ... on SiteFeatureGrid { title subtitle highlightedOnly }
  ... on SiteRichText { title body }
  ... on SiteCodeSample { title body language code }
  ... on SiteComparisonTeaser { title subtitle }
  ... on SiteFaq { title items { question answer } }
  ... on SiteCallToAction { title body primary { ${LINK} } secondary { ${LINK} } }
}`;

const PAGES_QUERY = `query WebsitePages {
  sitePages(first: 1000) { items { locale slug title description ${SECTIONS} } }
}`;

const COMPETITORS_QUERY = `query WebsiteCompetitors {
  siteCompetitors(orderBy: [order_ASC], first: 200) {
    items {
      locale name slug website kind summary rows { criterion tess other }
      chooseThem chooseTess migration importExportUrl migrationGuideUrl asOf order
    }
  }
}`;

export interface Content {
  settings: Settings;
  features: Feature[];
  pages: Page[];
  competitors: Competitor[];
}

/** What one language has: everything is optional, English fills the gaps. */
type LocaleContent = Partial<Content>;

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

function localeOf(entry: Localized): Locale {
  const value = entry.locale?.toLowerCase();
  return (LOCALES as readonly string[]).includes(value ?? "") ? (value as Locale) : DEFAULT_LOCALE;
}

function byLocale<T extends Localized>(entries: T[]): Record<Locale, T[]> {
  const out = Object.fromEntries(LOCALES.map((l) => [l, [] as T[]])) as Record<Locale, T[]>;
  for (const e of entries) out[localeOf(e)].push(e);
  return out;
}

async function fromTess(url: string): Promise<Record<Locale, LocaleContent>> {
  const [settings, features, pages, competitors] = await Promise.all([
    gql<{ siteSettingsEntries: { items: Settings[] } }>(url, SETTINGS_QUERY),
    gql<{ siteFeatures: { items: Feature[] } }>(url, FEATURES_QUERY),
    gql<{ sitePages: { items: Page[] } }>(url, PAGES_QUERY),
    gql<{ siteCompetitors: { items: Competitor[] } }>(url, COMPETITORS_QUERY),
  ]);
  const s = byLocale(settings.siteSettingsEntries.items);
  if (!s[DEFAULT_LOCALE][0]) throw new Error("Tess: publish one English SiteSettings entry (run `npm run seed`)");
  const f = byLocale(features.siteFeatures.items);
  const p = byLocale(pages.sitePages.items);
  const c = byLocale(competitors.siteCompetitors.items);
  return Object.fromEntries(
    LOCALES.map((l) => [l, { settings: s[l][0], features: f[l], pages: p[l], competitors: c[l] }]),
  ) as Record<Locale, LocaleContent>;
}

/** The seed files: content/seed.json (English) and content/seed.{locale}.json. */
const SEEDS: Record<Locale, LocaleContent> = {
  en: seed as unknown as LocaleContent,
  fr: seedFr as unknown as LocaleContent,
  zh: seedZh as unknown as LocaleContent,
  es: seedEs as unknown as LocaleContent,
};

/** English entries, replaced by the `locale` ones that exist (matched by slug). */
function mergeBySlug<T extends { slug: string }>(base: T[], localized: T[] | undefined): T[] {
  const own = new Map((localized ?? []).map((x) => [x.slug, x]));
  const merged = base.map((x) => own.get(x.slug) ?? x);
  const extra = (localized ?? []).filter((x) => !base.some((b) => b.slug === x.slug));
  return [...merged, ...extra];
}

function withFallback(all: Record<Locale, LocaleContent>, locale: Locale): Content {
  const en = all[DEFAULT_LOCALE] as Content;
  const own = all[locale] ?? {};
  return {
    settings: own.settings ?? en.settings,
    features: own.features?.length ? own.features : en.features,
    pages: mergeBySlug(en.pages, own.pages),
    competitors: mergeBySlug(en.competitors, own.competitors),
  };
}

let cache: Promise<Record<Locale, LocaleContent>> | undefined;

function allContent(): Promise<Record<Locale, LocaleContent>> {
  if (!cache) {
    const url = process.env.TESS_API_URL?.trim();
    if (url) {
      cache = fromTess(url);
    } else {
      console.warn("[website] TESS_API_URL is not set: building from content/seed*.json");
      cache = Promise.resolve(SEEDS);
    }
  }
  return cache;
}

/** The website content in `locale` (English where it has no translation), loaded once per build. */
export async function content(locale: Locale = DEFAULT_LOCALE): Promise<Content> {
  return withFallback(await allContent(), locale);
}

/** Data every page needs: settings for the layout. */
export interface LayoutData {
  settings: Settings;
}
