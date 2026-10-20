// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { PageContextServer } from "vike/types";
import { content } from "../../lib/content";
import { i18nFor, LOCALE_INFO } from "../../lib/i18n";

export type Data = Awaited<ReturnType<typeof data>>;

export async function data(pageContext: PageContextServer) {
  const { locale } = pageContext;
  const c = await content(locale);
  const { t } = i18nFor(locale);
  const competitors = [...c.competitors].sort((a, b) => (a.order ?? 0) - (b.order ?? 0));
  return {
    settings: c.settings,
    title: t("compare.metaTitle"),
    description: t("compare.metaDescription", {
      names: new Intl.ListFormat(LOCALE_INFO[locale].intl, { type: "conjunction" }).format(
        competitors.map((x) => x.name),
      ),
    }),
    competitors,
  };
}
