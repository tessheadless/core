// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useEffect, useId, useState, type ReactNode } from "react";

interface ModalProps {
  title: ReactNode;
  /** Called after the dialog closed (Esc, backdrop, close button or `close()`). */
  onClose: () => void;
  /** Render prop receiving a function that closes the dialog. */
  children: (close: () => void) => ReactNode;
  wide?: boolean;
  /** "drawer" slides in from the right, full height (detail panels). */
  variant?: "dialog" | "drawer";
}

/**
 * daisyUI modal built on the native <dialog>: showModal() gives us a focus trap,
 * Esc handling and focus restoration for free. Mount it to open it.
 */
export function Modal({ title, onClose, children, wide = false, variant = "dialog" }: ModalProps) {
  // Callback ref kept in state (not useRef) so `close` can be handed to children during render.
  const [dialog, setDialog] = useState<HTMLDialogElement | null>(null);
  const titleId = useId();

  useEffect(() => {
    if (dialog && !dialog.open) dialog.showModal();
  }, [dialog]);

  const close = () => dialog?.close();

  return (
    <dialog
      ref={setDialog}
      className={variant === "drawer" ? "modal modal-end" : "modal modal-bottom sm:modal-middle"}
      aria-labelledby={titleId}
      onClose={onClose}
    >
      <div
        className={
          variant === "drawer" ? "modal-box w-full sm:max-w-3xl" : `modal-box ${wide ? "sm:max-w-2xl" : ""}`.trim()
        }
      >
        <h2 id={titleId} className="pr-8 text-lg font-bold">
          {title}
        </h2>
        {children(close)}
        {/* Placed last so the first form field, not this button, gets the initial focus. */}
        <button
          type="button"
          className="btn btn-sm btn-circle btn-ghost absolute top-2 right-2"
          aria-label="Close dialog"
          onClick={close}
        >
          ✕
        </button>
      </div>
      <form method="dialog" className="modal-backdrop">
        <button type="submit" tabIndex={-1}>
          Close
        </button>
      </form>
    </dialog>
  );
}
