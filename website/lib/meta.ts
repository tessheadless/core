// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { Settings } from "./content";

/** What every `+data` hook returns: the layout settings and the page's SEO meta. */
export interface PageMeta {
  settings: Settings;
  title?: string;
  description?: string;
}

/** Internal links stay relative; others open as regular external links. */
export function isExternal(url: string): boolean {
  return /^[a-z][a-z0-9+.-]*:/i.test(url);
}
