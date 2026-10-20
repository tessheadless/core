// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { PageContext } from "vike/types";
import type { Data } from "./+data";

export function title(pageContext: PageContext<Data>) {
  return pageContext.data?.post.title ?? "Post";
}
