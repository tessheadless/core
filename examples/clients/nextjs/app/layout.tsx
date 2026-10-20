// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { Metadata } from "next";
import Link from "next/link";

export const metadata: Metadata = { title: "CMS blog (Next.js)" };

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body style={{ fontFamily: "system-ui, sans-serif", maxWidth: 720, margin: "2rem auto", padding: "0 1rem" }}>
        <nav>
          <Link href="/">Posts</Link> &middot; <Link href="/search">Search</Link>
        </nav>
        {children}
      </body>
    </html>
  );
}
