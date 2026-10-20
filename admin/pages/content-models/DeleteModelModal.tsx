// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useId, useState, type FormEvent } from "react";
import { Modal } from "../../components/Modal";
import { deleteContentType } from "../../lib/contentTypes";
import { describeError } from "../../lib/graphql";

interface DeleteModelModalProps {
  apiId: string;
  /** Content types with a relation pointing to this one (deletion is refused while they exist). */
  referencedBy: string[];
  onClose: () => void;
  onDeleted: (apiId: string) => void;
}

export function DeleteModelModal({ apiId, referencedBy, onClose, onDeleted }: DeleteModelModalProps) {
  const inputId = useId();
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const blocked = referencedBy.length > 0;

  async function submit(event: FormEvent, close: () => void) {
    event.preventDefault();
    if (typed !== apiId || blocked) return;
    setBusy(true);
    setError(null);
    try {
      await deleteContentType(apiId);
      onDeleted(apiId);
      close();
    } catch (e) {
      // Shown inline: toasts sit below the modal's top layer.
      setError(describeError(e));
      setBusy(false);
    }
  }

  return (
    <Modal title={`Delete ${apiId}?`} onClose={onClose}>
      {(close) => (
        <form onSubmit={(e) => void submit(e, close)}>
          {blocked ? (
            <div role="alert" className="alert alert-warning my-2 text-sm">
              <span>
                {referencedBy.map((r) => (
                  <code key={r} className="mr-1 font-semibold">
                    {r}
                  </code>
                ))}
                still {referencedBy.length > 1 ? "have relations" : "has a relation"} to <code>{apiId}</code>. Remove
                those fields first.
              </span>
            </div>
          ) : (
            <>
              <p className="py-2 text-sm">
                The content model <strong>and all of its entries</strong> (drafts and published) are deleted, and its
                queries and mutations disappear from the API. This cannot be undone.
              </p>
              <fieldset className="fieldset">
                <label className="fieldset-label" htmlFor={inputId}>
                  Type <code className="font-mono font-semibold">{apiId}</code> to confirm
                </label>
                <input
                  id={inputId}
                  type="text"
                  className="input w-full font-mono"
                  value={typed}
                  onChange={(e) => setTyped(e.target.value)}
                  autoComplete="off"
                  spellCheck={false}
                  disabled={busy}
                />
              </fieldset>
            </>
          )}
          {error && (
            <div role="alert" className="alert alert-error mt-3 text-sm">
              {error}
            </div>
          )}
          <div className="modal-action">
            <button type="button" className="btn" onClick={close}>
              Cancel
            </button>
            {!blocked && (
              <button type="submit" className="btn btn-error" disabled={busy || typed !== apiId}>
                {busy && <span className="loading loading-spinner loading-sm" aria-hidden="true" />}
                Delete content model
              </button>
            )}
          </div>
        </form>
      )}
    </Modal>
  );
}
