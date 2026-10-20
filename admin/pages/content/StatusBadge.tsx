// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { WorkflowStatus } from "../../lib/content";

const STYLES: Record<WorkflowStatus, { label: string; className: string }> = {
  DRAFT: { label: "Draft", className: "badge-ghost" },
  IN_REVIEW: { label: "In review", className: "badge-info" },
  CHANGES_REQUESTED: { label: "Changes requested", className: "badge-warning" },
  PUBLISHED: { label: "Published", className: "badge-success" },
};

export function StatusBadge({ status }: { status: WorkflowStatus }) {
  const { label, className } = STYLES[status] ?? STYLES.DRAFT;
  return <span className={`badge badge-sm whitespace-nowrap ${className}`}>{label}</span>;
}
