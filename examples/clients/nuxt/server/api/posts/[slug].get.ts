// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { Post, PostConnection } from "#shared/types/cms";

// GET /api/posts/:slug -> one post, looked up by slug (never by id).
export default defineEventHandler(async (event): Promise<Post> => {
  const slug = getRouterParam(event, "slug")!;
  const d = await cmsQuery<{ posts: PostConnection }>(
    `query PostBySlug($slug: String!) { posts(where: { slug: $slug }, first: 1) { total items { ${POST_CARD} body tags } } }`,
    { slug },
  );
  const post = d.posts.items[0];
  if (!post) throw createError({ statusCode: 404, statusMessage: "NOT_FOUND", message: `No post "${slug}"` });
  return post;
});
