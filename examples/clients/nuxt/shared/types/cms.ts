// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

export type Post = {
  /** Stable opaque reference: use as `:key` / cache key. Never use or persist `id` (encrypted, expires). */
  ref: string;
  title: string;
  slug: string;
  excerpt: string | null;
  body?: string | null;
  tags?: string[] | null;
  publishedAt: string | null;
  author: { name: string } | null;
};

export type PostConnection = { total: number; items: Post[] };
