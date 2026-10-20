// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { Plugin } from "../../lib/plugins";

export function StatusBadge({ plugin }: { plugin: Plugin }) {
  if (plugin.lastError) return <span className="badge badge-error">Error</span>;
  if (plugin.loaded) return <span className="badge badge-success">Running</span>;
  if (!plugin.enabled) return <span className="badge badge-ghost">Disabled</span>;
  return <span className="badge badge-warning">Not loaded</span>;
}
