// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { content } from "../../lib/content";

export type Data = Awaited<ReturnType<typeof data>>;

export async function data() {
  const c = await content();
  const competitors = [...c.competitors].sort((a, b) => (a.order ?? 0) - (b.order ?? 0));
  return {
    settings: c.settings,
    title: "Compare Tess with other CMSs",
    description: `How Tess compares with ${competitors.map((x) => x.name).join(", ")}: architecture, APIs, databases, workflows, hosting and license.`,
    competitors,
  };
}
