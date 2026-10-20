// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState, type FormEvent } from "react";
import { ConfirmModal } from "../../components/ConfirmModal";
import { ButtonSpinner, TextField } from "../../components/Field";
import { useToast } from "../../components/Toasts";
import { describeError } from "../../lib/graphql";
import { addLanguage, removeLanguage, updateLanguage, type OverviewData } from "../../lib/i18n";
import { useViewer } from "../../lib/viewer";

interface Props {
  overview: OverviewData;
  onChanged: () => void;
}

export function LanguagesTab({ overview, onChanged }: Props) {
  const toast = useToast();
  const { isAdmin } = useViewer();
  const [tag, setTag] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const stats = new Map(overview.translationStats.map((s) => [s.language, s]));

  async function add(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const l = await addLanguage(tag, name, false);
      toast.success(`${l.name} (${l.tag}) added.`);
      setTag("");
      setName("");
      onChanged();
    } catch (err) {
      setError(describeError(err));
    } finally {
      setBusy(false);
    }
  }

  async function makeBase(t: string) {
    try {
      await updateLanguage(t, undefined, true);
      toast.success(`${t} is now the base language.`);
      onChanged();
    } catch (e) {
      toast.error(describeError(e));
    }
  }

  return (
    <section aria-label="Languages" className="grid gap-8 lg:grid-cols-[2fr_1fr]">
      <div className="overflow-x-auto">
        <table className="table">
          <thead>
            <tr>
              <th>Language</th>
              <th>Progress</th>
              <th className="sr-only">Actions</th>
            </tr>
          </thead>
          <tbody>
            {overview.translationLanguages.map((l) => {
              const s = stats.get(l.tag);
              const pct = !s || s.total === 0 ? 100 : Math.round((s.translated / s.total) * 100);
              const reviewed = !s || s.total === 0 ? 100 : Math.round((s.reviewed / s.total) * 100);
              return (
                <tr key={l.tag}>
                  <td>
                    <div className="font-semibold">{l.name}</div>
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-sm">{l.tag}</span>
                      {l.base && <span className="badge badge-sm badge-primary">base</span>}
                    </div>
                  </td>
                  <td className="min-w-48">
                    {l.base ? (
                      <span className="text-base-content/70 text-sm">
                        {s?.translated ?? 0} / {s?.total ?? 0} keys written
                      </span>
                    ) : (
                      <div className="flex flex-col gap-1 text-xs">
                        <progress
                          className="progress progress-success"
                          value={pct}
                          max={100}
                          aria-label={`${pct}% translated`}
                        />
                        <span className="text-base-content/70">
                          {pct}% translated · {reviewed}% reviewed
                        </span>
                      </div>
                    )}
                  </td>
                  <td className="text-right">
                    {isAdmin && (
                      <div className="flex justify-end gap-1">
                        {!l.base && (
                          <button type="button" className="btn btn-xs btn-ghost" onClick={() => void makeBase(l.tag)}>
                            Make base
                          </button>
                        )}
                        <button
                          type="button"
                          className="btn btn-xs btn-ghost text-error"
                          onClick={() => setRemoving(l.tag)}
                        >
                          Remove
                        </button>
                      </div>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {overview.translationLanguages.length === 0 && (
          <p className="text-base-content/60 py-6 text-center">No language yet.</p>
        )}
      </div>

      {isAdmin && (
        <form onSubmit={(e) => void add(e)} className="card bg-base-200 h-fit">
          <div className="card-body">
            <h2 className="card-title text-lg">Add a language</h2>
            <TextField
              label="Tag"
              value={tag}
              onChange={setTag}
              mono
              required
              placeholder="fr-CA"
              hint="BCP 47: en, de, pt-BR, zh-Hant…"
            />
            <TextField label="Name" value={name} onChange={setName} placeholder="Français (Canada)" />
            {error && <p className="text-error text-sm">{error}</p>}
            <p className="text-base-content/70 text-xs">The first language added becomes the base language.</p>
            <button type="submit" className="btn btn-primary mt-2" disabled={busy || !tag.trim()}>
              <ButtonSpinner busy={busy} />
              Add language
            </button>
          </div>
        </form>
      )}

      {removing && (
        <ConfirmModal
          title={`Remove ${removing}?`}
          confirmLabel="Remove language"
          onClose={() => setRemoving(null)}
          onConfirm={async () => {
            await removeLanguage(removing);
            toast.success(`${removing} removed.`);
            onChanged();
          }}
        >
          Every <span className="font-mono">{removing}</span> translation is deleted. This cannot be undone.
        </ConfirmModal>
      )}
    </section>
  );
}
