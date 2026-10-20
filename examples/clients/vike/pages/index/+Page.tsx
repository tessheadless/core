// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useData } from "vike-react/useData";
import type { Data } from "./+data";

export default function Page() {
  const { posts } = useData<Data>();
  return (
    <main>
      <h1>Posts</h1>
      <ul>
        {posts.map((p) => (
          // `ref` is the stable key; links use the slug.
          <li key={p.ref}>
            <a href={`/posts/${p.slug}`}>{p.title}</a>
            {p.author && <> by {p.author.name}</>}
            {p.excerpt && <p>{p.excerpt}</p>}
          </li>
        ))}
      </ul>
    </main>
  );
}
