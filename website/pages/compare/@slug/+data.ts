// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { render } from "vike/abort";
import type { PageContextServer } from "vike/types";
import { content } from "../../../lib/content";

export type Data = Awaited<ReturnType<typeof data>>;

export async function data(pageContext: PageContextServer) {
  const c = await content();
  const competitor = c.competitors.find((x) => x.slug === pageContext.routeParams.slug);
  if (!competitor) throw render(404, "Unknown comparison");
  const others = c.competitors.filter((x) => x.slug !== competitor.slug).map(({ name, slug }) => ({ name, slug }));
  return {
    settings: c.settings,
    title: `Tess vs ${competitor.name}`,
    description: `Tess compared with ${competitor.name} (${competitor.kind}): ${competitor.summary}`,
    competitor,
    others,
  };
}
