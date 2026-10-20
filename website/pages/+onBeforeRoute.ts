// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { PageContext } from "vike/types";
import { extractLocale } from "../lib/i18n";

/** `/fr/features` is routed as `/features`, with `pageContext.locale = "fr"` (server and browser). */
export function onBeforeRoute(pageContext: PageContext) {
  const { locale, path } = extractLocale(pageContext.urlPathname);
  return { pageContext: { locale, urlLogical: path } };
}
