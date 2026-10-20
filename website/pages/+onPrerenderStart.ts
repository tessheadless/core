// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { PageContextServer } from "vike/types";
import { DEFAULT_LOCALE, LOCALES } from "../lib/i18n";

/** Every page is prerendered once per language: `/features`, `/fr/features`, `/zh/features`… */
export function onPrerenderStart(prerenderContext: { pageContexts: PageContextServer[] }) {
  const pageContexts: PageContextServer[] = [];
  for (const pageContext of prerenderContext.pageContexts) {
    for (const locale of LOCALES) {
      const urlOriginal =
        locale === DEFAULT_LOCALE
          ? pageContext.urlOriginal
          : `/${locale}${pageContext.urlOriginal === "/" ? "/" : pageContext.urlOriginal}`;
      pageContexts.push({ ...pageContext, urlOriginal, locale });
    }
  }
  return { prerenderContext: { pageContexts } };
}
