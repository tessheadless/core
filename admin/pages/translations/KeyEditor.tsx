// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState } from "react";
import { ButtonSpinner, TextField } from "../../components/Field";
import { Modal } from "../../components/Modal";
import { useToast } from "../../components/Toasts";
import { formatDate } from "../../lib/format";
import { describeError } from "../../lib/graphql";
import { setTranslation, updateKey, type Language, type TranslationKey } from "../../lib/i18n";
import { useViewer } from "../../lib/viewer";
import { StateBadge } from "./StateBadge";

interface Props {
  translationKey: TranslationKey;
  languages: Language[];
  namespaces: string[];
  onClose: () => void;
  onSaved: (k: TranslationKey) => void;
  /** The id changes when the name or namespace changes. */
  onRenamed: (k: TranslationKey, previousId: string) => void;
}

/** Drawer editing one key: its metadata and the text of every language. */
export function KeyEditor({ translationKey: k, languages, namespaces, onClose, onSaved, onRenamed }: Props) {
  const toast = useToast();
  const { translations: can } = useViewer();
  const [meta, setMeta] = useState({
    name: k.name,
    namespace: k.namespace,
    description: k.description ?? "",
    tags: k.tags.join(", "),
    plural: k.plural,
  });
  const [metaBusy, setMetaBusy] = useState(false);
  const [metaError, setMetaError] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>(() =>
    Object.fromEntries(languages.map((l) => [l.tag, k.translations.find((t) => t.language === l.tag)?.text ?? ""])),
  );
  const [busy, setBusy] = useState<string | null>(null);
  const base = languages.find((l) => l.base);
  const baseText = base ? k.translations.find((t) => t.language === base.tag)?.text : undefined;

  const metaDirty =
    meta.name !== k.name ||
    meta.namespace !== k.namespace ||
    meta.description !== (k.description ?? "") ||
    meta.tags !== k.tags.join(", ") ||
    meta.plural !== k.plural;

  async function saveMeta() {
    setMetaBusy(true);
    setMetaError(null);
    try {
      const updated = await updateKey(k.id, {
        name: meta.name.trim(),
        namespace: meta.namespace.trim(),
        description: meta.description.trim() || null,
        tags: meta.tags
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
        plural: meta.plural,
      });
      if (updated.id !== k.id) onRenamed(updated, k.id);
      else onSaved(updated);
      toast.success("Key saved.");
    } catch (e) {
      setMetaError(describeError(e));
    } finally {
      setMetaBusy(false);
    }
  }

  async function save(tag: string, state?: "REVIEWED" | "TRANSLATED", textOnly = true) {
    setBusy(tag);
    try {
      const current = k.translations.find((t) => t.language === tag);
      const text = drafts[tag] ?? "";
      const changed = text !== (current?.text ?? "");
      const updated = await setTranslation(k.id, tag, changed || textOnly ? text : undefined, state);
      onSaved(updated);
    } catch (e) {
      toast.error(`${tag}: ${describeError(e)}`);
    } finally {
      setBusy(null);
    }
  }

  return (
    <Modal title={<span className="font-mono break-all">{k.name}</span>} onClose={onClose} variant="drawer">
      {() => (
        <div className="mt-4 flex flex-col gap-6">
          {k.description && <p className="bg-base-200 rounded p-3 text-sm">{k.description}</p>}

          <div className="flex flex-col gap-4">
            {languages.map((l) => {
              const current = k.translations.find((t) => t.language === l.tag);
              const dirty = (drafts[l.tag] ?? "") !== (current?.text ?? "");
              const isBase = l.tag === base?.tag;
              return (
                <fieldset key={l.tag} className="fieldset">
                  <legend className="fieldset-legend flex w-full items-center gap-2">
                    <span className="font-mono">{l.tag}</span>
                    <span className="text-base-content/70 font-normal">{l.name}</span>
                    {isBase ? (
                      <span className="badge badge-xs badge-ghost">base</span>
                    ) : (
                      <StateBadge state={current?.state ?? null} />
                    )}
                    {current && (
                      <span className="text-base-content/50 ml-auto text-xs font-normal">
                        {current.updatedBy} · {formatDate(current.updatedAt)}
                      </span>
                    )}
                  </legend>
                  <textarea
                    className="textarea w-full font-mono text-sm"
                    rows={Math.min(8, Math.max(2, (drafts[l.tag] ?? "").split("\n").length))}
                    value={drafts[l.tag] ?? ""}
                    placeholder={
                      !isBase && baseText ? baseText : k.plural ? "{count, plural, one {# item} other {# items}}" : ""
                    }
                    onChange={(e) => setDrafts((d) => ({ ...d, [l.tag]: e.target.value }))}
                    readOnly={!can.canEdit}
                    aria-label={`${l.name} text`}
                  />
                  <div className="flex flex-wrap gap-2">
                    {can.canEdit && (
                      <button
                        type="button"
                        className="btn btn-xs btn-primary"
                        disabled={!dirty || busy !== null}
                        onClick={() => void save(l.tag)}
                      >
                        <ButtonSpinner busy={busy === l.tag} />
                        {drafts[l.tag] === "" && current ? "Remove" : "Save"}
                      </button>
                    )}
                    {can.canReview && !isBase && current && current.state !== "REVIEWED" && (
                      <button
                        type="button"
                        className="btn btn-xs btn-success btn-outline"
                        disabled={busy !== null}
                        onClick={() => void save(l.tag, "REVIEWED", false)}
                      >
                        {dirty ? "Save as reviewed" : "Mark reviewed"}
                      </button>
                    )}
                    {can.canReview && !isBase && current?.state === "REVIEWED" && !dirty && (
                      <button
                        type="button"
                        className="btn btn-xs btn-ghost"
                        disabled={busy !== null}
                        onClick={() => void save(l.tag, "TRANSLATED", false)}
                      >
                        Back to review
                      </button>
                    )}
                    {dirty && (
                      <button
                        type="button"
                        className="btn btn-xs btn-ghost"
                        onClick={() => setDrafts((d) => ({ ...d, [l.tag]: current?.text ?? "" }))}
                      >
                        Revert
                      </button>
                    )}
                  </div>
                </fieldset>
              );
            })}
          </div>

          {can.canEdit && (
            <details className="collapse-arrow bg-base-200 collapse">
              <summary className="collapse-title font-semibold">Key settings</summary>
              <div className="collapse-content">
                <div className="grid gap-2 sm:grid-cols-2">
                  <TextField
                    label="Name"
                    value={meta.name}
                    onChange={(name) => setMeta((m) => ({ ...m, name }))}
                    mono
                  />
                  <TextField
                    label="Namespace"
                    value={meta.namespace}
                    onChange={(namespace) => setMeta((m) => ({ ...m, namespace }))}
                    mono
                    list="i18n-namespaces"
                    hint="Empty = default namespace."
                  />
                </div>
                <datalist id="i18n-namespaces">
                  {namespaces.filter(Boolean).map((ns) => (
                    <option key={ns} value={ns}>
                      {ns}
                    </option>
                  ))}
                </datalist>
                <TextField
                  label="Description"
                  value={meta.description}
                  onChange={(description) => setMeta((m) => ({ ...m, description }))}
                  hint="Context for translators: where the text appears, length limits…"
                />
                <TextField
                  label="Tags"
                  value={meta.tags}
                  onChange={(tags) => setMeta((m) => ({ ...m, tags }))}
                  hint="Comma-separated."
                />
                <label className="label mt-2 cursor-pointer justify-start gap-2">
                  <input
                    type="checkbox"
                    className="checkbox checkbox-sm"
                    checked={meta.plural}
                    onChange={(e) => setMeta((m) => ({ ...m, plural: e.target.checked }))}
                  />
                  Plural (ICU <code>{"{count, plural, …}"}</code>, exported as plural forms where supported)
                </label>
                {metaError && <p className="text-error mt-2 text-sm">{metaError}</p>}
                <button
                  type="button"
                  className="btn btn-sm btn-primary mt-3"
                  disabled={!metaDirty || metaBusy}
                  onClick={() => void saveMeta()}
                >
                  <ButtonSpinner busy={metaBusy} />
                  Save key settings
                </button>
              </div>
            </details>
          )}
        </div>
      )}
    </Modal>
  );
}
