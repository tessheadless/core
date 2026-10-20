// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import Link from "next/link";
import { listPosts } from "@/lib/cms";

export const revalidate = 60;

export default async function Home() {
  const { total, items } = await listPosts();
  return (
    <main>
      <h1>Posts ({total})</h1>
      <ul>
        {items.map((p) => (
          <li key={p.ref}>
            <Link href={`/posts/${p.slug}`}>{p.title}</Link>
            {p.author && <> by {p.author.name}</>}
            {p.excerpt && <p>{p.excerpt}</p>}
          </li>
        ))}
      </ul>
    </main>
  );
}
