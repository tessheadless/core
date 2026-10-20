// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { content } from "../../../lib/content";

export async function onBeforePrerenderStart() {
  const { competitors } = await content();
  return competitors.map((c) => `/compare/${c.slug}`);
}
