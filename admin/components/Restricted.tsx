// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { ReactNode } from "react";
import { withBase } from "./Link";

/** Renders `children` only when `allowed`; otherwise explains why (pages are reachable by URL). */
export function Restricted({ allowed, what, children }: { allowed: boolean; what: string; children: ReactNode }) {
  if (allowed) return children;
  return (
    <div className="py-12 text-center">
      <h1 className="mb-2 text-3xl font-bold">Access restricted</h1>
      <p className="text-base-content/70 mb-6">Your role does not allow you to {what}.</p>
      <a href={withBase("/")} className="btn btn-primary">
        Back to the dashboard
      </a>
    </div>
  );
}
