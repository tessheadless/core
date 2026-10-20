// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useEffect, useState } from "react";
import { ConfirmModal } from "../../components/ConfirmModal";
import { useToast } from "../../components/Toasts";
import { describeError } from "../../lib/graphql";
import {
  deleteKeys,
  fetchKeys,
  type KeyFilter,
  type OverviewData,
  type StateFilter,
  type TranslationKey,
} from "../../lib/i18n";
import { useViewer } from "../../lib/viewer";
import { KeyEditor } from "./KeyEditor";
import { MoveKeysModal } from "./MoveKeysModal";
import { NewKeyModal } from "./NewKeyModal";
import { StateBadge } from "./StateBadge";

const PAGE = 50;

interface Props {
  overview: OverviewData;
  onChanged: () => void;
}

export function KeysTab({ overview, onChanged }: Props) {
  const toast = useToast();
  const { translations: can } = useViewer();
  const languages = overview.translationLanguages;
  const base = languages.find((l) => l.base)?.tag;
  const [filter, setFilter] = useState<KeyFilter>({});
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  const [result, setResult] = useState<{ items: TranslationKey[]; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Which request the shown result answers; loading while it differs from the current one.
  const [answered, setAnswered] = useState<string | null>(null);
  const [version, setVersion] = useState(0);
  const [columns, setColumns] = useState<string[]>(() => languages.slice(0, 3).map((l) => l.tag));
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [editing, setEditing] = useState<TranslationKey | null>(null);
  const [creating, setCreating] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [moving, setMoving] = useState(false);

  // Debounced search box.
  useEffect(() => {
    const t = window.setTimeout(() => {
      setFilter((f) => ({ ...f, search: search.trim() || undefined }));
      setPage(0);
    }, 250);
    return () => window.clearTimeout(t);
  }, [search]);

  const request = JSON.stringify([filter, page, version]);
  const loading = answered !== request;
  useEffect(() => {
    let cancelled = false;
    fetchKeys(filter, PAGE, page * PAGE).then(
      (r) => {
        if (cancelled) return;
        setResult(r);
        setError(null);
        setAnswered(request);
      },
      (e: unknown) => {
        if (cancelled) return;
        setError(describeError(e));
        setAnswered(request);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [filter, page, version, request]);

  const refresh = () => {
    setVersion((v) => v + 1);
    onChanged();
  };

  function replaceKey(updated: TranslationKey, previousId = updated.id) {
    setResult((r) => r && { ...r, items: r.items.map((k) => (k.id === previousId ? updated : k)) });
    setEditing((e) => (e && e.id === previousId ? updated : e));
  }

  const update = (patch: Partial<KeyFilter>) => {
    setFilter((f) => ({ ...f, ...patch }));
    setPage(0);
  };
  const pages = result ? Math.max(1, Math.ceil(result.total / PAGE)) : 1;
  const allOnPage = result?.items.map((k) => k.id) ?? [];

  return (
    <section aria-label="Translation keys">
      <div className="mb-4 flex flex-wrap items-end gap-2">
        <label className="input input-sm w-64">
          <span className="sr-only">Search</span>
          <input
            type="search"
            placeholder="Search keys and texts"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </label>
        <select
          className="select select-sm w-40"
          aria-label="Namespace"
          value={filter.namespace ?? "*"}
          onChange={(e) => update({ namespace: e.target.value === "*" ? undefined : e.target.value })}
        >
          <option value="*">All namespaces</option>
          {overview.translationNamespaces.map((ns) => (
            <option key={ns} value={ns}>
              {ns || "(default)"}
            </option>
          ))}
        </select>
        {overview.translationTags.length > 0 && (
          <select
            className="select select-sm w-36"
            aria-label="Tag"
            value={filter.tag ?? ""}
            onChange={(e) => update({ tag: e.target.value || undefined })}
          >
            <option value="">All tags</option>
            {overview.translationTags.map((t) => (
              <option key={t}>{t}</option>
            ))}
          </select>
        )}
        <select
          className="select select-sm w-40"
          aria-label="State"
          value={filter.state ?? ""}
          onChange={(e) => update({ state: (e.target.value || undefined) as StateFilter | undefined })}
        >
          <option value="">Any state</option>
          <option value="UNTRANSLATED">Untranslated</option>
          <option value="TRANSLATED">To review</option>
          <option value="REVIEWED">Reviewed</option>
        </select>
        {filter.state && (
          <select
            className="select select-sm w-36"
            aria-label="State in language"
            value={filter.language ?? ""}
            onChange={(e) => update({ language: e.target.value || undefined })}
          >
            <option value="">in any language</option>
            {languages.map((l) => (
              <option key={l.tag} value={l.tag}>
                in {l.tag}
              </option>
            ))}
          </select>
        )}
        <details className="dropdown">
          <summary className="btn btn-sm btn-ghost">Columns ({columns.length})</summary>
          <ul className="dropdown-content menu bg-base-100 rounded-box z-10 w-52 p-2 shadow">
            {languages.map((l) => (
              <li key={l.tag}>
                <label className="flex gap-2">
                  <input
                    type="checkbox"
                    className="checkbox checkbox-xs"
                    checked={columns.includes(l.tag)}
                    onChange={(e) =>
                      setColumns((c) =>
                        e.target.checked
                          ? languages.map((x) => x.tag).filter((t) => t === l.tag || c.includes(t))
                          : c.filter((t) => t !== l.tag),
                      )
                    }
                  />
                  <span className="font-mono">{l.tag}</span> {l.name}
                </label>
              </li>
            ))}
          </ul>
        </details>
        <div className="ml-auto flex gap-2">
          {can.canEdit && selected.size > 0 && (
            <button type="button" className="btn btn-sm btn-outline" onClick={() => setMoving(true)}>
              Change namespace ({selected.size})
            </button>
          )}
          {can.canEdit && selected.size > 0 && (
            <button type="button" className="btn btn-sm btn-error btn-outline" onClick={() => setConfirmDelete(true)}>
              Delete {selected.size}
            </button>
          )}
          {can.canEdit && languages.length > 0 && (
            <button type="button" className="btn btn-sm btn-primary" onClick={() => setCreating(true)}>
              New key
            </button>
          )}
        </div>
      </div>

      {error && (
        <div role="alert" className="alert alert-error mb-4">
          <span>{error}</span>
        </div>
      )}

      <div className="overflow-x-auto">
        <table className="table-sm table">
          <thead>
            <tr>
              {can.canEdit && (
                <th className="w-8">
                  <input
                    type="checkbox"
                    className="checkbox checkbox-xs"
                    aria-label="Select all keys on this page"
                    checked={allOnPage.length > 0 && allOnPage.every((id) => selected.has(id))}
                    onChange={(e) => setSelected(e.target.checked ? new Set(allOnPage) : new Set())}
                  />
                </th>
              )}
              <th>Key</th>
              {columns.map((tag) => (
                <th key={tag} className="min-w-48">
                  <span className="font-mono">{tag}</span>
                  {tag === base && <span className="badge badge-xs badge-ghost ml-1">base</span>}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {result?.items.map((k) => (
              <tr key={k.id} className="hover:bg-base-200 cursor-pointer" onClick={() => setEditing(k)}>
                {can.canEdit && (
                  <td onClick={(e) => e.stopPropagation()}>
                    <input
                      type="checkbox"
                      className="checkbox checkbox-xs"
                      aria-label={`Select ${k.name}`}
                      checked={selected.has(k.id)}
                      onChange={(e) =>
                        setSelected((s) => {
                          const next = new Set(s);
                          if (e.target.checked) next.add(k.id);
                          else next.delete(k.id);
                          return next;
                        })
                      }
                    />
                  </td>
                )}
                <td className="align-top">
                  <button
                    type="button"
                    className="link link-hover text-left font-mono text-sm break-all"
                    onClick={() => setEditing(k)}
                  >
                    {k.name}
                  </button>
                  <div className="mt-1 flex flex-wrap gap-1">
                    {k.namespace && <span className="badge badge-xs badge-outline">{k.namespace}</span>}
                    {k.plural && <span className="badge badge-xs badge-info badge-outline">plural</span>}
                    {k.tags.map((t) => (
                      <span key={t} className="badge badge-xs badge-ghost">
                        {t}
                      </span>
                    ))}
                  </div>
                </td>
                {columns.map((tag) => {
                  const t = k.translations.find((x) => x.language === tag);
                  return (
                    <td key={tag} className="align-top">
                      {t ? (
                        <div className="flex items-start gap-2">
                          <span className="line-clamp-3 text-sm whitespace-pre-wrap">{t.text}</span>
                          {tag !== base && <StateBadge state={t.state} />}
                        </div>
                      ) : (
                        <span className="text-base-content/40 text-sm italic">untranslated</span>
                      )}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
        {result && result.items.length === 0 && (
          <p className="text-base-content/60 py-8 text-center">
            {filter.search || filter.namespace || filter.state || filter.tag
              ? "No key matches these filters."
              : "No translation key yet."}
          </p>
        )}
      </div>

      <div className="mt-4 flex items-center justify-between">
        <span className="text-base-content/70 text-sm">
          {loading && <span className="loading loading-spinner loading-xs mr-2" aria-hidden="true" />}
          {result ? `${result.total} key${result.total === 1 ? "" : "s"}` : ""}
        </span>
        {pages > 1 && (
          <div className="join">
            <button
              type="button"
              className="btn btn-sm join-item"
              disabled={page === 0}
              onClick={() => setPage((p) => p - 1)}
            >
              «
            </button>
            <span className="btn btn-sm join-item pointer-events-none">
              {page + 1} / {pages}
            </span>
            <button
              type="button"
              className="btn btn-sm join-item"
              disabled={page + 1 >= pages}
              onClick={() => setPage((p) => p + 1)}
            >
              »
            </button>
          </div>
        )}
      </div>

      {editing && (
        <KeyEditor
          key={editing.id}
          translationKey={editing}
          languages={languages}
          namespaces={overview.translationNamespaces}
          onClose={() => setEditing(null)}
          onSaved={replaceKey}
          onRenamed={(updated, previousId) => {
            replaceKey(updated, previousId);
            onChanged();
          }}
        />
      )}
      {creating && (
        <NewKeyModal
          languages={languages}
          namespaces={overview.translationNamespaces}
          defaultNamespace={filter.namespace}
          onClose={() => setCreating(false)}
          onCreated={(k) => {
            toast.success(`Key “${k.name}” created.`);
            refresh();
            setEditing(k);
          }}
        />
      )}
      {moving && (
        <MoveKeysModal
          ids={[...selected]}
          namespaces={overview.translationNamespaces}
          onClose={() => setMoving(false)}
          onMoved={(n, ns) => {
            // Moved keys get new ids.
            setSelected(new Set());
            toast.success(`${n} key${n === 1 ? "" : "s"} moved to ${ns ? `“${ns}”` : "the default namespace"}.`);
            refresh();
          }}
        />
      )}
      {confirmDelete && (
        <ConfirmModal
          title={`Delete ${selected.size} key${selected.size === 1 ? "" : "s"}?`}
          confirmLabel="Delete"
          onClose={() => setConfirmDelete(false)}
          onConfirm={async () => {
            const n = await deleteKeys([...selected]);
            setSelected(new Set());
            toast.success(`${n} key${n === 1 ? "" : "s"} deleted.`);
            refresh();
          }}
        >
          The keys and all their translations are removed. This cannot be undone.
        </ConfirmModal>
      )}
    </section>
  );
}
