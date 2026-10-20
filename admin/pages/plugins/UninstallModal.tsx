// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useId, useState, type FormEvent } from "react";
import { Modal } from "../../components/Modal";
import { useToast } from "../../components/Toasts";
import { describeError } from "../../lib/graphql";
import { uninstallPlugin } from "../../lib/plugins";

interface UninstallModalProps {
  name: string;
  onClose: () => void;
  onUninstalled: (name: string) => void;
}

export function UninstallModal({ name, onClose, onUninstalled }: UninstallModalProps) {
  const toast = useToast();
  const inputId = useId();
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(event: FormEvent, close: () => void) {
    event.preventDefault();
    if (typed !== name) return;
    setBusy(true);
    setError(null);
    try {
      await uninstallPlugin(name);
      onUninstalled(name);
      toast.success(`Plugin "${name}" uninstalled.`);
      close();
    } catch (e) {
      // Shown inline: toasts sit below the modal's top layer.
      setError(describeError(e));
      setBusy(false);
    }
  }

  return (
    <Modal title={`Uninstall ${name}?`} onClose={onClose}>
      {(close) => (
        <form onSubmit={(e) => void submit(e, close)}>
          <p className="py-2 text-sm">
            The module and its settings are deleted. Content writes will no longer go through this plugin. This cannot
            be undone.
          </p>
          <fieldset className="fieldset">
            <label className="fieldset-label" htmlFor={inputId}>
              Type <code className="font-mono font-semibold">{name}</code> to confirm
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
          {error && (
            <div role="alert" className="alert alert-error mt-3 text-sm">
              {error}
            </div>
          )}
          <div className="modal-action">
            <button type="button" className="btn" onClick={close}>
              Cancel
            </button>
            <button type="submit" className="btn btn-error" disabled={busy || typed !== name}>
              {busy && <span className="loading loading-spinner loading-sm" aria-hidden="true" />}
              Uninstall
            </button>
          </div>
        </form>
      )}
    </Modal>
  );
}
