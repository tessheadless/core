// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { FeatureIcon } from "../lib/content";

// Outline icons (24×24, stroke), one per value of the SiteFeature `icon` enum.
const PATHS: Record<FeatureIcon, string> = {
  BOLT: "M13 2 4 14h7l-1 8 9-12h-7l1-8z",
  GRAPH:
    "M6 6m-2 0a2 2 0 1 0 4 0a2 2 0 1 0-4 0M18 6m-2 0a2 2 0 1 0 4 0a2 2 0 1 0-4 0M12 18m-2 0a2 2 0 1 0 4 0a2 2 0 1 0-4 0M8 6h8M7 8l4 8M17 8l-4 8",
  DATABASE:
    "M4 6c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zM4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3",
  LOCK: "M6 11h12v10H6zM8 11V7a4 4 0 0 1 8 0v4",
  PLUG: "M9 2v6M15 2v6M6 8h12v3a6 6 0 0 1-12 0zM12 17v5",
  CLOUD: "M7 18a5 5 0 0 1-.6-9.96A6 6 0 0 1 18 9a4.5 4.5 0 0 1-.5 9z",
  IMAGE: "M3 5h18v14H3zM3 16l5-5 4 4 3-3 6 6M15.5 9.5m-1.5 0a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0-3 0",
  SEARCH: "M11 11m-7 0a7 7 0 1 0 14 0a7 7 0 1 0-14 0M16 16l5 5",
  USERS: "M9 8m-4 0a4 4 0 1 0 8 0a4 4 0 1 0-8 0M2 21a7 7 0 0 1 14 0M16 4a4 4 0 0 1 0 8M22 21a7 7 0 0 0-4-6.3",
  WORKFLOW: "M4 4h6v6H4zM14 14h6v6h-6zM7 10v4a3 3 0 0 0 3 3h4M17 14V7l-2 2M17 7l2 2",
  CODE: "M8 8l-4 4 4 4M16 8l4 4-4 4M14 4l-4 16",
  ROBOT: "M5 8h14v11H5zM12 4v4M9 13h.01M15 13h.01M9 16h6M2 12v3M22 12v3",
};

export function Icon({ name, className = "size-6" }: { name: FeatureIcon | null; className?: string }) {
  const d = PATHS[name ?? "BOLT"] ?? PATHS.BOLT;
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      <path d={d} />
    </svg>
  );
}
