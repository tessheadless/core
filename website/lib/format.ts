// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { LOCALE_INFO, type Locale } from "./i18n";

/** `2026-10-20` → `October 20, 2026` in `locale` (UTC, so prerendering is deterministic). */
export function formatDate(date: string, locale: Locale): string {
  const d = new Date(`${date}T00:00:00Z`);
  if (Number.isNaN(d.getTime())) return date;
  return d.toLocaleDateString(LOCALE_INFO[locale].intl, {
    year: "numeric",
    month: "long",
    day: "numeric",
    timeZone: "UTC",
  });
}
