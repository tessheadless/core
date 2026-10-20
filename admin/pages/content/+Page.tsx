// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useEffect, useMemo, useState } from "react";
import { BUILTIN_TYPES, fetchModels, type ContentType } from "../../lib/contentTypes";
import { listEntries, type EntrySummary, type Models } from "../../lib/content";
import { formatDate } from "../../lib/format";
import { describeError } from "../../lib/graphql";
import { useQuery } from "../../lib/useQuery";
import { useViewer } from "../../lib/viewer";
import { EntryEditor } from "./EntryEditor";
import { StatusBadge } from "./StatusBadge";

const PAGE_SIZE = 25;

async function loadModels(): Promise<{ list: ContentType[]; models: Models }> {
  const { contentTypes, components } = await fetchModels();
  return {
    // Assets are created by uploads (relation pickers), not edited here.
    list: contentTypes.filter((t) => !BUILTIN_TYPES.includes(t.apiId)),
    models: {
      types: new Map(contentTypes.map((t) => [t.apiId, t])),
      components: new Map(components.map((c) => [c.apiId, c])),
    },
  };
}

/** The selected type lives in `?type=` so the view can be bookmarked and reloaded. */
function initialType(): string | null {
  if (typeof window === "undefined") return null;
  return new URLSearchParams(window.location.search).get("type");
}

export default function Page() {
  const { data, error, loading } = useQuery(loadModels);
  const [selected, setSelected] = useState<string | null>(initialType);

  const type = data?.list.find((t) => t.apiId === selected) ?? data?.list[0] ?? null;

  function select(apiId: string) {
    setSelected(apiId);
    const url = new URL(window.location.href);
    url.searchParams.set("type", apiId);
    window.history.replaceState(null, "", url);
  }

  return (
    <>
      <h1 className="mb-6 text-3xl font-bold">Content</h1>
      {error && (
        <div role="alert" className="alert alert-error mb-4">
          Could not load the content models: {error}
        </div>
      )}
      {loading && !data && (
        <div className="flex justify-center py-12">
          <span className="loading loading-spinner loading-lg" aria-label="Loading content models" />
        </div>
      )}
      {data && data.list.length === 0 && (
        <p className="text-base-content/70">No content type yet. An admin can create one in Content models.</p>
      )}
      {data && type && (
        <div className="flex flex-col gap-6 lg:flex-row">
          <nav aria-label="Content types" className="lg:w-56 lg:shrink-0">
            <ul className="menu bg-base-200 rounded-box w-full">
              {data.list.map((t) => (
                <li key={t.apiId}>
                  <button
                    type="button"
                    className={t.apiId === type.apiId ? "menu-active" : undefined}
                    aria-current={t.apiId === type.apiId ? "true" : undefined}
                    onClick={() => select(t.apiId)}
                  >
                    {t.apiId}
                  </button>
                </li>
              ))}
            </ul>
          </nav>
          <section className="min-w-0 flex-1">
            <EntryList key={type.apiId} type={type} models={data.models} />
          </section>
        </div>
      )}
    </>
  );
}

function EntryList({ type, models }: { type: ContentType; models: Models }) {
  const { content } = useViewer();
  const [search, setSearch] = useState("");
  const [query, setQuery] = useState("");
  const [skip, setSkip] = useState(0);
  const [version, setVersion] = useState(0);
  const [page, setPage] = useState<{ total: number; items: EntrySummary[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Key of the last finished request: loading while it differs from the current one.
  const [doneKey, setDoneKey] = useState<string | null>(null);
  const [editing, setEditing] = useState<{ id: string | null } | null>(null);
  const canEdit = content.canEdit(type.apiId);
  const hasLabel = useMemo(() => type.fields.some((f) => f.type === "TEXT" || f.type === "MARKDOWN"), [type]);

  // Debounced search.
  useEffect(() => {
    const t = window.setTimeout(() => {
      setQuery(search);
      setSkip(0);
    }, 300);
    return () => window.clearTimeout(t);
  }, [search]);

  const key = `${skip}|${query}|${version}`;
  const loading = doneKey !== key;

  useEffect(() => {
    let cancelled = false;
    listEntries(type, { first: PAGE_SIZE, skip, search: query }).then(
      (p) => {
        if (cancelled) return;
        setPage(p);
        setError(null);
        setDoneKey(key);
      },
      (e: unknown) => {
        if (cancelled) return;
        setError(describeError(e));
        setDoneKey(key);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [type, skip, query, key]);

  const refresh = () => setVersion((v) => v + 1);
  const last = page ? Math.min(skip + PAGE_SIZE, page.total) : 0;

  return (
    <>
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <div className="mr-auto">
          <h2 className="text-xl font-semibold">{type.pluralApiId}</h2>
          <p className="text-base-content/70 text-sm">
            {type.workflow ? "Review workflow enabled." : "Published directly."}
            {!canEdit && " Read only for your role."}
          </p>
        </div>
        {hasLabel && (
          <input
            type="search"
            className="input input-sm w-48"
            placeholder="Search…"
            aria-label={`Search ${type.pluralApiId}`}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        )}
        <button type="button" className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
          {loading && <span className="loading loading-spinner loading-xs" aria-hidden="true" />}
          Refresh
        </button>
        {canEdit && (
          <button type="button" className="btn btn-primary btn-sm" onClick={() => setEditing({ id: null })}>
            New {type.apiId}
          </button>
        )}
      </div>

      {error && (
        <div role="alert" className="alert alert-error mb-4">
          {error}
        </div>
      )}

      {page && (
        <div className="rounded-box border-base-300 overflow-x-auto border">
          <table className="table">
            <thead>
              <tr>
                <th>Entry</th>
                <th>Status</th>
                <th>Updated</th>
                <th>By</th>
              </tr>
            </thead>
            <tbody>
              {page.items.length === 0 && (
                <tr>
                  <td colSpan={4} className="text-base-content/70 py-8 text-center">
                    {query ? "No entry matches this search." : "No entry yet."}
                  </td>
                </tr>
              )}
              {page.items.map((e) => (
                <tr key={e.ref} className="hover:bg-base-200">
                  <td>
                    <button
                      type="button"
                      className="link link-hover text-left font-medium"
                      onClick={() => setEditing({ id: e.id })}
                    >
                      {e.label || <span className="font-mono text-xs">{e.ref}</span>}
                    </button>
                  </td>
                  <td>
                    <div className="flex flex-wrap gap-1">
                      <StatusBadge status={e.workflowStatus} />
                      {e.publishedAt && e.workflowStatus !== "PUBLISHED" && (
                        <span className="badge badge-sm badge-success badge-outline whitespace-nowrap">live</span>
                      )}
                    </div>
                  </td>
                  <td className="whitespace-nowrap">{formatDate(e.updatedAt)}</td>
                  <td>{e.updatedBy ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {page && page.total > PAGE_SIZE && (
        <div className="mt-4 flex items-center justify-between text-sm">
          <span>
            {skip + 1}–{last} of {page.total}
          </span>
          <div className="join">
            <button
              type="button"
              className="btn btn-sm join-item"
              disabled={skip === 0 || loading}
              onClick={() => setSkip(Math.max(0, skip - PAGE_SIZE))}
            >
              Previous
            </button>
            <button
              type="button"
              className="btn btn-sm join-item"
              disabled={last >= page.total || loading}
              onClick={() => setSkip(skip + PAGE_SIZE)}
            >
              Next
            </button>
          </div>
        </div>
      )}

      {editing && (
        <EntryEditor
          type={type}
          models={models}
          entryId={editing.id}
          onClose={() => setEditing(null)}
          onChanged={refresh}
        />
      )}
    </>
  );
}
