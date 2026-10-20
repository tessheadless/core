// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { PageContextServer } from "vike/types";
import { render } from "vike/abort";
import { postBySlug } from "../../../lib/cms";

export async function data(pageContext: PageContextServer) {
  const post = await postBySlug(pageContext.routeParams.slug);
  if (!post) throw render(404, `No post with slug "${pageContext.routeParams.slug}"`);
  return { post };
}
export type Data = Awaited<ReturnType<typeof data>>;
