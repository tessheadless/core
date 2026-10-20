// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import path from "node:path";
import type { GatsbyNode } from "gatsby";

// Build-time only: TESS_URL comes from the build environment.
const TESS_URL = (process.env.TESS_URL ?? "http://localhost:8080").replace(/\/+$/, "");

type CmsPost = {
  ref: string;
  title: string;
  slug: string | null;
  excerpt: string | null;
  body: string | null;
  tags: string[] | null;
  createdAt: string;
  publishedAt: string | null;
  author: { name: string } | null;
};

type GraphQLResponse<T> = { data?: T; errors?: { message: string; extensions?: { code?: string } }[] };

async function cmsQuery<T>(query: string, variables: Record<string, unknown> = {}): Promise<T> {
  const res = await fetch(`${TESS_URL}/graphql`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ query, variables }),
  });
  if (!res.ok) throw new Error(`CMS HTTP ${res.status}: ${await res.text()}`);
  const json = (await res.json()) as GraphQLResponse<T>;
  if (json.errors?.length) {
    const [e] = json.errors;
    throw new Error(`CMS error [${e.extensions?.code ?? "UNKNOWN"}] ${e.message}`);
  }
  return json.data as T;
}

const PAGE_SIZE = 100;

// 1. Pull every published post (paginated with first/skip) into Gatsby's data layer.
export const sourceNodes: GatsbyNode["sourceNodes"] = async ({ actions, createNodeId, createContentDigest, reporter }) => {
  let skip = 0;
  let total = Infinity;
  while (skip < total) {
    const { posts } = await cmsQuery<{ posts: { total: number; items: CmsPost[] } }>(
      `query Posts($first: Int!, $skip: Int!) {
        posts(orderBy: [createdAt_DESC], first: $first, skip: $skip) {
          total
          items { ref title slug excerpt body tags createdAt publishedAt author { name } }
        }
      }`,
      { first: PAGE_SIZE, skip },
    );
    total = posts.total;
    for (const post of posts.items) {
      // Node ids derive from the stable `ref` (never from the expiring `id`),
      // so Gatsby's cache stays valid across builds.
      actions.createNode({
        ...post,
        id: createNodeId(`CmsPost-${post.ref}`),
        internal: { type: "CmsPost", contentDigest: createContentDigest(post) },
      });
    }
    if (posts.items.length === 0) break;
    skip += posts.items.length;
  }
  reporter.info(`[tess] sourced ${total} post(s) from ${TESS_URL}`);
};

// 2. Explicit schema so builds work even when a field is null everywhere.
export const createSchemaCustomization: GatsbyNode["createSchemaCustomization"] = ({ actions }) => {
  actions.createTypes(`
    type CmsAuthor { name: String! }
    type CmsPost implements Node @dontInfer {
      ref: String!
      title: String!
      slug: String
      excerpt: String
      body: String
      tags: [String!]
      createdAt: Date! @dateformat
      publishedAt: Date @dateformat
      author: CmsAuthor
    }
  `);
};

// 3. One page per slug (slugs are not enforced unique by the CMS: first wins).
export const createPages: GatsbyNode["createPages"] = async ({ graphql, actions, reporter }) => {
  const result = await graphql<{ allCmsPost: { nodes: { slug: string | null }[] } }>(`
    { allCmsPost(sort: { createdAt: DESC }) { nodes { slug } } }
  `);
  if (result.errors) {
    reporter.panicOnBuild("could not query CmsPost nodes", result.errors);
    return;
  }
  const seen = new Set<string>();
  for (const { slug } of result.data!.allCmsPost.nodes) {
    if (!slug || seen.has(slug)) continue;
    seen.add(slug);
    actions.createPage({
      path: `/posts/${slug}/`,
      component: path.resolve("src/templates/post.tsx"),
      context: { slug },
    });
  }
};
