// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { NextResponse, type NextRequest } from "next/server";
import { CmsError, searchPosts } from "@/lib/cms";

// GET /api/search?q=lambda -> { total, items: [{ ref, title, slug, ... }] }
// A JSON proxy for client-side widgets: the browser never talks to the CMS directly.
export async function GET(req: NextRequest) {
  const q = req.nextUrl.searchParams.get("q")?.trim();
  if (!q) return NextResponse.json({ error: "missing ?q=", code: "BAD_USER_INPUT" }, { status: 400 });
  try {
    return NextResponse.json(await searchPosts(q));
  } catch (e) {
    const code = e instanceof CmsError ? e.code : "INTERNAL";
    const status = code === "BAD_USER_INPUT" ? 400 : 502;
    return NextResponse.json({ error: "search failed", code }, { status });
  }
}
