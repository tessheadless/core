// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { ROLE_LABEL, type Role } from "../lib/accounts";

const ROLE_CLASS: Record<Role, string> = {
  OWNER: "badge-primary",
  ADMIN: "badge-secondary",
  CONTRIBUTOR: "badge-info",
  REVIEWER: "badge-accent",
  MEMBER: "badge-ghost",
};

export function RoleBadge({ role, size = "sm" }: { role: Role; size?: "xs" | "sm" | "md" }) {
  const sizeClass = size === "xs" ? "badge-xs" : size === "sm" ? "badge-sm" : "";
  return <span className={`badge ${ROLE_CLASS[role]} ${sizeClass} whitespace-nowrap`}>{ROLE_LABEL[role]}</span>;
}

/** "All content types" or the list of apiIds. */
export function ScopeList({ scope }: { scope: string[] }) {
  if (scope.length === 0) return <span className="badge badge-ghost badge-sm">All</span>;
  return (
    <div className="flex flex-wrap gap-1">
      {scope.map((apiId) => (
        <span key={apiId} className="badge badge-outline badge-sm font-mono">
          {apiId}
        </span>
      ))}
    </div>
  );
}
