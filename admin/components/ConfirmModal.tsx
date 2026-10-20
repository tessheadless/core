// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState, type ReactNode } from "react";
import { describeError } from "../lib/graphql";
import { Modal } from "./Modal";

interface ConfirmModalProps {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  /** Runs the action; throwing keeps the dialog open and shows the error inline. */
  onConfirm: () => Promise<void>;
  onClose: () => void;
}

/** Yes/no dialog for destructive actions. */
export function ConfirmModal({ title, children, confirmLabel, onConfirm, onClose }: ConfirmModalProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function confirm(close: () => void) {
    setBusy(true);
    setError(null);
    try {
      await onConfirm();
      close();
    } catch (e) {
      // Shown inline: toasts sit below the modal's top layer.
      setError(describeError(e));
      setBusy(false);
    }
  }

  return (
    <Modal title={title} onClose={onClose}>
      {(close) => (
        <>
          <div className="py-2 text-sm">{children}</div>
          {error && (
            <div role="alert" className="alert alert-error mt-3 text-sm">
              {error}
            </div>
          )}
          <div className="modal-action">
            <button type="button" className="btn" onClick={close}>
              Cancel
            </button>
            <button type="button" className="btn btn-error" disabled={busy} onClick={() => void confirm(close)}>
              {busy && <span className="loading loading-spinner loading-sm" aria-hidden="true" />}
              {confirmLabel}
            </button>
          </div>
        </>
      )}
    </Modal>
  );
}
