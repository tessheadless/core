// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { listPosts } from "../../lib/cms";

// Runs on the server / at prerender time only.
export async function data() {
  return { posts: await listPosts() };
}
export type Data = Awaited<ReturnType<typeof data>>;
