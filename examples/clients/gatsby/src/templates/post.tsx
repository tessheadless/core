// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import * as React from "react";
import { graphql, Link, type HeadProps, type PageProps } from "gatsby";

type Data = {
  cmsPost: {
    title: string;
    body: string | null;
    tags: string[] | null;
    publishedAt: string | null;
    author: { name: string } | null;
  } | null;
};

export const query = graphql`
  query PostBySlug($slug: String!) {
    cmsPost(slug: { eq: $slug }) {
      title
      body
      tags
      publishedAt(formatString: "MMMM D, YYYY")
      author { name }
    }
  }
`;

export default function PostTemplate({ data }: PageProps<Data>) {
  const post = data.cmsPost;
  if (!post) return <p>Not found</p>;
  return (
    <article style={{ fontFamily: "system-ui, sans-serif", maxWidth: 720, margin: "2rem auto", padding: "0 1rem" }}>
      <p>
        <Link to="/">&larr; All posts</Link>
      </p>
      <h1>{post.title}</h1>
      <p>
        {post.author?.name ?? "Unknown author"}
        {post.publishedAt && <> &middot; {post.publishedAt}</>}
      </p>
      {post.tags?.length ? <p>Tags: {post.tags.join(", ")}</p> : null}
      <div style={{ whiteSpace: "pre-wrap" }}>{post.body}</div>
    </article>
  );
}

export const Head = ({ data }: HeadProps<Data>) => <title>{data.cmsPost?.title ?? "Post"}</title>;
