// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { PageContext } from "vike/types";
import type { PageMeta } from "../lib/meta";

export default function description(pageContext: PageContext): string {
  const data = pageContext.data as PageMeta | undefined;
  return data?.description ?? data?.settings.description ?? "";
}
