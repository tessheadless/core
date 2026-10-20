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

/** Links to other sites (`https://…`, `mailto:`…); the website and the docs use relative links. */
export function isExternal(url: string): boolean {
  return /^[a-z][a-z0-9+.-]*:/i.test(url);
}

/** Where the documentation is served, on the same domain as the website. */
export const DOCS_PATH = "/docs/";

/**
 * Same-domain links outside the website (the docs): a full page load, not the
 * website's client-side navigation (`rel="external"` tells Vike).
 */
export function isOtherApp(url: string): boolean {
  return url === DOCS_PATH.slice(0, -1) || url.startsWith(DOCS_PATH);
}
