// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import * as React from "react";
import { graphql, Link, type HeadFC, type PageProps } from "gatsby";

type Data = {
  allCmsPost: {
    totalCount: number;
    nodes: { ref: string; title: string; slug: string | null; excerpt: string | null; author: { name: string } | null }[];
  };
};

export const query = graphql`
  query PostList {
    allCmsPost(sort: { createdAt: DESC }) {
      totalCount
      nodes { ref title slug excerpt author { name } }
    }
  }
`;

export default function IndexPage({ data }: PageProps<Data>) {
  return (
    <main style={{ fontFamily: "system-ui, sans-serif", maxWidth: 720, margin: "2rem auto", padding: "0 1rem" }}>
      <h1>Posts ({data.allCmsPost.totalCount})</h1>
      <ul>
        {data.allCmsPost.nodes.map((p) => (
          // `ref` is the stable key; links use the slug.
          <li key={p.ref}>
            {p.slug ? <Link to={`/posts/${p.slug}/`}>{p.title}</Link> : p.title}
            {p.author && <> by {p.author.name}</>}
            {p.excerpt && <p>{p.excerpt}</p>}
          </li>
        ))}
      </ul>
    </main>
  );
}

export const Head: HeadFC = () => <title>CMS blog (Gatsby)</title>;
