// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { PageContext } from "vike/types";
import type { PageMeta } from "../lib/meta";

export default function title(pageContext: PageContext): string {
  const data = pageContext.data as PageMeta | undefined;
  const site = data?.settings.siteName ?? "Tess";
  if (!data?.title) return site;
  return data.title.startsWith(site) ? data.title : `${data.title} · ${site}`;
}
