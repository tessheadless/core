// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { render } from "vike/abort";
import type { PageContextServer } from "vike/types";
import { content } from "../../../lib/content";
import { i18nFor } from "../../../lib/i18n";

export type Data = Awaited<ReturnType<typeof data>>;

export async function data(pageContext: PageContextServer) {
  const { locale } = pageContext;
  const c = await content(locale);
  const { t } = i18nFor(locale);
  const competitor = c.competitors.find((x) => x.slug === pageContext.routeParams.slug);
  if (!competitor) throw render(404, "Unknown comparison");
  const others = c.competitors.filter((x) => x.slug !== competitor.slug).map(({ name, slug }) => ({ name, slug }));
  return {
    settings: c.settings,
    title: t("compare.tessVs", { name: competitor.name }),
    description: t("compare.competitorDescription", {
      name: competitor.name,
      kind: competitor.kind,
      summary: competitor.summary,
    }),
    competitor,
    others,
  };
}
