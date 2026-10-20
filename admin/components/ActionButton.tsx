// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { ReactNode } from "react";

interface ActionButtonProps {
  reason: string | null;
  danger?: boolean;
  onClick: () => void;
  children: ReactNode;
}

/** Small table action; when not allowed it is disabled and the reason is shown as a tooltip and to screen readers. */
export function ActionButton({ reason, danger = false, onClick, children }: ActionButtonProps) {
  const button = (
    <button
      type="button"
      className={`btn btn-xs ${danger ? "btn-error btn-outline" : "btn-ghost"}`}
      onClick={onClick}
      disabled={reason !== null}
    >
      {children}
      {reason && <span className="sr-only"> (unavailable: {reason})</span>}
    </button>
  );
  return reason ? (
    <span className="tooltip tooltip-left" data-tip={reason}>
      {button}
    </span>
  ) : (
    button
  );
}
