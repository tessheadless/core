// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { render } from "vike/abort";
import type { SectionContext } from "../components/Sections";
import { content, type Page } from "./content";
import type { Locale } from "./i18n";
import type { PageMeta } from "./meta";

export interface SitePageData extends PageMeta {
  page: Page;
  context: SectionContext;
}

/** Data of a SitePage by slug, in `locale` (404 when it does not exist). */
export async function sitePageData(slug: string, locale: Locale): Promise<SitePageData> {
  const c = await content(locale);
  const page = c.pages.find((p) => p.slug === slug);
  if (!page) throw render(404, `No page \`${slug}\``);
  return {
    settings: c.settings,
    title: page.title,
    description: page.description,
    page,
    context: {
      features: [...c.features].sort((a, b) => (a.order ?? 0) - (b.order ?? 0)),
      competitors: c.competitors.map(({ name, slug, kind, summary }) => ({ name, slug, kind, summary })),
    },
  };
}
