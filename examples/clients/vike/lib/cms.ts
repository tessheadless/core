// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// Server-side GraphQL helper. It only runs at build time (prerendering), so
// TESS_URL is read from the build environment and never shipped to the browser.

export type Post = {
  /** Stable opaque reference: use it as React key / cache key. (Never use `id`: it is encrypted and expires.) */
  ref: string;
  title: string;
  slug: string;
  excerpt: string | null;
  body?: string | null;
  tags?: string[] | null;
  publishedAt: string | null;
  author: { name: string } | null;
};

export class CmsError extends Error {
  constructor(
    public code: string,
    message: string,
  ) {
    super(`[${code}] ${message}`);
  }
}

const TESS_URL = (process.env.TESS_URL ?? "http://localhost:8080").replace(/\/+$/, "");

export async function gql<T>(query: string, variables: Record<string, unknown> = {}): Promise<T> {
  const res = await fetch(`${TESS_URL}/graphql`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ query, variables }),
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

export async function listPosts(): Promise<Post[]> {
  const data = await gql<{ posts: { items: Post[] } }>(
    `query { posts(orderBy: [createdAt_DESC], first: 100) { total items { ${CARD} } } }`,
  );
  return data.posts.items;
}

/** Look posts up by slug (stable, human readable), never by id. */
export async function postBySlug(slug: string): Promise<Post | null> {
  const data = await gql<{ posts: { items: Post[] } }>(
    `query PostBySlug($slug: String!) {
      posts(where: { slug: $slug }, first: 1) { items { ${CARD} body tags } }
    }`,
    { slug },
  );
  return data.posts.items[0] ?? null;
}
