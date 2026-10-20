// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { listPosts, postBySlug } from "@/lib/cms";

export const revalidate = 60;
// Slugs published after the build are rendered on first request, then cached.
export const dynamicParams = true;

type Props = { params: Promise<{ slug: string }> };

export async function generateStaticParams() {
  const { items } = await listPosts(100);
  // Slugs are not enforced unique by the CMS: de-duplicate.
  return [...new Set(items.map((p) => p.slug).filter(Boolean))].map((slug) => ({ slug }));
}

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const post = await postBySlug((await params).slug);
  return { title: post?.title ?? "Not found", description: post?.excerpt ?? undefined };
}

export default async function PostPage({ params }: Props) {
  const post = await postBySlug((await params).slug);
  if (!post) notFound();
  return (
    <article>
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
