// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { PostConnection } from "#shared/types/cms";

// GET /api/posts            -> latest published posts
// GET /api/posts?q=lambda   -> full-text search (searchPosts)
export default defineEventHandler(async (event) => {
  const q = String(getQuery(event).q ?? "").trim();
  if (q) {
    const d = await cmsQuery<{ searchPosts: PostConnection }>(
      `query Search($q: String!) { searchPosts(query: $q, first: 20) { total items { ${POST_CARD} } } }`,
      { q },
    );
    return d.searchPosts;
  }
  const d = await cmsQuery<{ posts: PostConnection }>(
    `query { posts(orderBy: [createdAt_DESC], first: 50) { total items { ${POST_CARD} } } }`,
  );
  return d.posts;
});
