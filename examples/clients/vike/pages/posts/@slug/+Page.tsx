// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useData } from "vike-react/useData";
import type { Data } from "./+data";

export default function Page() {
  const { post } = useData<Data>();
  return (
    <article>
      <p>
        <a href="/">&larr; All posts</a>
      </p>
      <h1>{post.title}</h1>
      <p>
        {post.author?.name ?? "Unknown author"}
        {post.publishedAt && <> &middot; {new Date(post.publishedAt).toLocaleDateString("en")}</>}
      </p>
      {post.tags?.length ? <p>Tags: {post.tags.join(", ")}</p> : null}
      <div style={{ whiteSpace: "pre-wrap" }}>{post.body}</div>
    </article>
  );
}
