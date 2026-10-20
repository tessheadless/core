// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { TranslationState } from "../../lib/i18n";

export function StateBadge({ state }: { state: TranslationState | null }) {
  if (state === "REVIEWED") return <span className="badge badge-xs badge-success shrink-0">reviewed</span>;
  if (state === "TRANSLATED") return <span className="badge badge-xs badge-warning shrink-0">to review</span>;
  return <span className="badge badge-xs badge-ghost shrink-0">untranslated</span>;
}
