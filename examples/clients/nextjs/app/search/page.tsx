// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import Link from "next/link";
import { CmsError, searchPosts } from "@/lib/cms";

type Props = { searchParams: Promise<{ q?: string }> };

// Rendered per request (depends on ?q=); the CMS call itself is still cached 60 s per query.
export default async function SearchPage({ searchParams }: Props) {
  const q = ((await searchParams).q ?? "").trim();
  let result: Awaited<ReturnType<typeof searchPosts>> | null = null;
  let error: string | null = null;
  if (q) {
    try {
      result = await searchPosts(q);
    } catch (e) {
      error = e instanceof CmsError ? e.message : "Search failed";
    }
  }
  return (
    <main>
      <h1>Search</h1>
      <form>
        <input name="q" defaultValue={q} placeholder="Search posts" />
        <button type="submit">Search</button>
      </form>
      {error && <p role="alert">{error}</p>}
      {result && (
        <>
          <p>
            {result.total} result(s) for &ldquo;{q}&rdquo;
          </p>
          <ul>
            {result.items.map((p) => (
              <li key={p.ref}>
                <Link href={`/posts/${p.slug}`}>{p.title}</Link>
              </li>
            ))}
          </ul>
        </>
      )}
    </main>
  );
}
