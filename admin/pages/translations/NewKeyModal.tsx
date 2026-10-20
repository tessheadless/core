// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState, type FormEvent } from "react";
import { ButtonSpinner, TextField } from "../../components/Field";
import { Modal } from "../../components/Modal";
import { describeError } from "../../lib/graphql";
import { createKey, type Language, type TranslationKey } from "../../lib/i18n";

interface Props {
  languages: Language[];
  namespaces: string[];
  defaultNamespace?: string;
  onClose: () => void;
  onCreated: (k: TranslationKey) => void;
}

export function NewKeyModal({ languages, namespaces, defaultNamespace, onClose, onCreated }: Props) {
  const base = languages.find((l) => l.base) ?? languages[0];
  const [name, setName] = useState("");
  const [namespace, setNamespace] = useState(defaultNamespace ?? "");
  const [description, setDescription] = useState("");
  const [text, setText] = useState("");
  const [plural, setPlural] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: FormEvent, close: () => void) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const k = await createKey({
        name: name.trim(),
        namespace: namespace.trim(),
        description: description.trim() || null,
        plural,
        translations: text && base ? [{ language: base.tag, text }] : [],
      });
      close();
      onCreated(k);
    } catch (err) {
      setError(describeError(err));
      setBusy(false);
    }
  }

  return (
    <Modal title="New translation key" onClose={onClose}>
      {(close) => (
        <form onSubmit={(e) => void submit(e, close)} className="mt-2">
          <TextField label="Key" value={name} onChange={setName} mono required placeholder="home.hero.title" />
          <TextField
            label="Namespace"
            value={namespace}
            onChange={setNamespace}
            mono
            list="new-key-namespaces"
            hint="Optional, e.g. common, checkout."
          />
          <datalist id="new-key-namespaces">
            {namespaces.filter(Boolean).map((ns) => (
              <option key={ns} value={ns}>
                {ns}
              </option>
            ))}
          </datalist>
          <TextField
            label="Description"
            value={description}
            onChange={setDescription}
            hint="Context for translators."
          />
          {base && (
            <fieldset className="fieldset">
              <label className="fieldset-label" htmlFor="new-key-text">
                {base.name} text ({base.tag})
              </label>
              <textarea
                id="new-key-text"
                className="textarea w-full font-mono"
                rows={3}
                value={text}
                onChange={(e) => setText(e.target.value)}
              />
            </fieldset>
          )}
          <label className="label mt-2 cursor-pointer justify-start gap-2">
            <input
              type="checkbox"
              className="checkbox checkbox-sm"
              checked={plural}
              onChange={(e) => setPlural(e.target.checked)}
            />
            Plural message
          </label>
          {error && <p className="text-error mt-2 text-sm">{error}</p>}
          <div className="modal-action">
            <button type="button" className="btn btn-ghost" onClick={close}>
              Cancel
            </button>
            <button type="submit" className="btn btn-primary" disabled={busy || !name.trim()}>
              <ButtonSpinner busy={busy} />
              Create
            </button>
          </div>
        </form>
      )}
    </Modal>
  );
}
