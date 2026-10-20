// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { listPosts } from "../../../lib/cms";

// Tells the prerenderer which /posts/@slug URLs exist. Slugs are plain fields
// (not enforced unique by the CMS), so de-duplicate them.
export async function onBeforePrerenderStart() {
  const posts = await listPosts();
  return [...new Set(posts.map((p) => p.slug).filter(Boolean))].map((slug) => `/posts/${slug}`);
}
