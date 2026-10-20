// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { sitePageData } from "../../lib/pageData";

export type Data = Awaited<ReturnType<typeof data>>;

/** The home page is the SitePage with slug `home`. */
export async function data() {
  return sitePageData("home");
}
