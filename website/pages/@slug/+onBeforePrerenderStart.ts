// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { content } from "../../lib/content";

/** Every SitePage except `home` (served at /) becomes /{slug}. */
export async function onBeforePrerenderStart() {
  const { pages } = await content();
  return pages.filter((p) => p.slug !== "home").map((p) => `/${p.slug}`);
}
