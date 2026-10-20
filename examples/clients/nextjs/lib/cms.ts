// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import "server-only";

// Server-only GraphQL helper: TESS_URL (and any token) never reach the browser.
const TESS_URL = (process.env.TESS_URL ?? "http://localhost:8080").replace(/\/+$/, "");

export type Post = {
  /** Stable opaque reference: use as React key / cache key. Never use or persist `id` (encrypted, expires). */
  ref: string;
  title: string;
  slug: string;
  excerpt: string | null;
  body?: string | null;
  tags?: string[] | null;
  publishedAt: string | null;
  author: { name: string } | null;
};

type Connection = { total: number; items: Post[] };

export class CmsError extends Error {
  constructor(
    public code: string,
    message: string,
  ) {
    super(`[${code}] ${message}`);
  }
}

type FetchOpts = { revalidate?: number | false; tags?: string[] };

export async function gql<T>(
  query: string,
  variables: Record<string, unknown> = {},
  { revalidate = 60, tags }: FetchOpts = {},
): Promise<T> {
  const res = await fetch(`${TESS_URL}/graphql`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ query, variables }),
    // ISR: cached by the Next.js data cache and refreshed at most every 60 s.
    next: { revalidate, tags },
  });
  if (!res.ok) throw new CmsError(`HTTP_${res.status}`, await res.text());
  const json = (await res.json()) as { data?: T; errors?: { message: string; extensions?: { code?: string } }[] };
  if (json.errors?.length) {
    const [e] = json.errors;
    throw new CmsError(e.extensions?.code ?? "UNKNOWN", e.message);
  }
  return json.data as T;
}

const CARD = "ref title slug excerpt publishedAt author { name }";

export async function listPosts(first = 50): Promise<Connection> {
  const d = await gql<{ posts: Connection }>(
    `query ListPosts($first: Int!) { posts(orderBy: [createdAt_DESC], first: $first) { total items { ${CARD} } } }`,
    { first },
    { tags: ["posts"] },
  );
  return d.posts;
}

/** Route by slug (stable, readable), never by id. */
export async function postBySlug(slug: string): Promise<Post | null> {
  const d = await gql<{ posts: Connection }>(
    `query PostBySlug($slug: String!) { posts(where: { slug: $slug }, first: 1) { total items { ${CARD} body tags } } }`,
    { slug },
    { tags: ["posts", `post:${slug}`] },
  );
  return d.posts.items[0] ?? null;
}

export async function searchPosts(query: string, first = 20): Promise<Connection> {
  const d = await gql<{ searchPosts: Connection }>(
    `query Search($q: String!, $first: Int!) { searchPosts(query: $q, first: $first) { total items { ${CARD} } } }`,
    { q: query, first },
    { revalidate: 60, tags: ["posts"] },
  );
  return d.searchPosts;
}
