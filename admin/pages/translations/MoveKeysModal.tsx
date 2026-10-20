// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState, type FormEvent } from "react";
import { ButtonSpinner, TextField } from "../../components/Field";
import { Modal } from "../../components/Modal";
import { describeError } from "../../lib/graphql";
import { moveKeys } from "../../lib/i18n";

interface Props {
  ids: string[];
  namespaces: string[];
  onClose: () => void;
  onMoved: (moved: number, namespace: string) => void;
}

/** Bulk action: changes the namespace of the selected keys. */
export function MoveKeysModal({ ids, namespaces, onClose, onMoved }: Props) {
  const [namespace, setNamespace] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const count = `${ids.length} key${ids.length === 1 ? "" : "s"}`;

  async function submit(e: FormEvent, close: () => void) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const ns = namespace.trim();
      const moved = await moveKeys(ids, ns);
      close();
      onMoved(moved, ns);
    } catch (err) {
      setError(describeError(err));
      setBusy(false);
    }
  }

  return (
    <Modal title={`Move ${count}`} onClose={onClose}>
      {(close) => (
        <form onSubmit={(e) => void submit(e, close)} className="mt-2">
          <TextField
            label="New namespace"
            value={namespace}
            onChange={setNamespace}
            mono
            list="move-keys-namespaces"
            hint="An existing or new namespace; empty for the default one."
          />
          <datalist id="move-keys-namespaces">
            {namespaces.filter(Boolean).map((ns) => (
              <option key={ns} value={ns}>
                {ns}
              </option>
            ))}
          </datalist>
          <p className="text-base-content/70 mt-2 text-sm">
            Translations move with their keys. Apps that load keys by namespace (e.g. i18next files) find them in the
            new one. Nothing moves if the namespace already has a key of the same name.
          </p>
          {error && <p className="text-error mt-2 text-sm">{error}</p>}
          <div className="modal-action">
            <button type="button" className="btn btn-ghost" onClick={close}>
              Cancel
            </button>
            <button type="submit" className="btn btn-primary" disabled={busy}>
              <ButtonSpinner busy={busy} />
              Move
            </button>
          </div>
        </form>
      )}
    </Modal>
  );
}
