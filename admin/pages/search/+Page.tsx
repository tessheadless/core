// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState } from "react";
import { ButtonSpinner } from "../../components/Field";
import { Restricted } from "../../components/Restricted";
import { useToast } from "../../components/Toasts";
import { describeError } from "../../lib/graphql";
import {
  fetchSearchSettings,
  reindexSearch,
  updateSearchSettings,
  type IndexType,
  type SearchModel,
} from "../../lib/search";
import { useQuery } from "../../lib/useQuery";
import { useViewer } from "../../lib/viewer";

const INDEX_LABELS: Record<IndexType, string> = {
  TEXT: "Full-text",
  KEYWORD: "Exact value, facet",
  KEYWORDS: "List of values, facet",
  INT: "Integer, sortable",
  FLOAT: "Number, sortable",
  BOOL: "Boolean, facet",
  TIMESTAMP: "Unix seconds, sortable",
};

export default function Page() {
  const { isAdmin } = useViewer();
  return (
    <Restricted allowed={isAdmin} what="manage search">
      <Search />
    </Restricted>
  );
}

function Search() {
  const { data, error, loading, reload, setData } = useQuery(fetchSearchSettings);

  function replace(model: SearchModel) {
    setData((d) => d && { ...d, models: d.models.map((m) => (m.apiId === model.apiId ? model : m)) });
  }

  return (
    <>
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-3xl font-bold">Search</h1>
        <div className="flex items-center gap-2">
          {data && <span className="badge badge-outline">{data.engine}</span>}
          <button type="button" className="btn btn-ghost btn-sm" onClick={reload} disabled={loading}>
            Refresh
          </button>
        </div>
      </div>
      <p className="text-base-content/70 mb-6 max-w-3xl text-sm">
        Enable full-text search per model and pick the indexed fields. Entries are pushed to the search engine whenever
        they are created, updated, published or deleted, and the engine schema of each field follows from its type.
        Enabled models get a <code>search&lt;Plural&gt;</code> GraphQL query that searches their text fields.
      </p>

      {error && (
        <div role="alert" className="alert alert-error mb-4">
          <span>Could not load the search settings: {error}</span>
        </div>
      )}
      {data === null && (
        <div role="alert" className="alert alert-info">
          <span>
            No search engine is configured on this server. Set <code>TESS_SEARCH=typesense</code>,{" "}
            <code>TYPESENSE_URL</code> and <code>TYPESENSE_API_KEY</code>, then restart Tess.
          </span>
        </div>
      )}
      {data && data.models.length === 0 && <p className="text-base-content/60 py-8 text-center">No model yet.</p>}
      <div className="flex flex-col gap-4">
        {data?.models.map((m) => (
          <ModelCard key={m.apiId} model={m} engine={data.engine} onSaved={replace} />
        ))}
      </div>
    </>
  );
}

function ModelCard({
  model,
  engine,
  onSaved,
}: {
  model: SearchModel;
  engine: string;
  onSaved: (m: SearchModel) => void;
}) {
  const toast = useToast();
  const [enabled, setEnabled] = useState(model.enabled);
  const [selected, setSelected] = useState<ReadonlySet<string>>(
    () => new Set(model.fields.filter((f) => f.selected).map((f) => f.name)),
  );
  const [busy, setBusy] = useState<"save" | "reindex" | null>(null);
  const [error, setError] = useState<string | null>(null);

  const saved = new Set(model.fields.filter((f) => f.selected).map((f) => f.name));
  const dirty = enabled !== model.enabled || selected.size !== saved.size || [...selected].some((f) => !saved.has(f));
  const hasText = model.fields.some((f) => f.searchable && selected.has(f.name));
  const searchableFields = model.fields.filter((f) => f.searchable);

  function toggleEnabled(on: boolean) {
    setEnabled(on);
    // First time: start from the text fields.
    if (on && selected.size === 0) setSelected(new Set(searchableFields.map((f) => f.name)));
  }

  function toggleField(name: string, on: boolean) {
    setSelected((s) => {
      const next = new Set(s);
      if (on) next.add(name);
      else next.delete(name);
      return next;
    });
  }

  async function save() {
    setBusy("save");
    setError(null);
    try {
      const fields = model.fields.filter((f) => selected.has(f.name)).map((f) => f.name);
      const m = await updateSearchSettings(model.apiId, enabled, fields);
      onSaved(m);
      toast.success(
        m.enabled
          ? `${model.apiId}: search enabled, ${m.indexedEntries ?? 0} entr${m.indexedEntries === 1 ? "y" : "ies"} indexed.`
          : `${model.apiId}: search disabled, index removed.`,
      );
    } catch (e) {
      setError(describeError(e));
    } finally {
      setBusy(null);
    }
  }

  async function reindex() {
    setBusy("reindex");
    setError(null);
    try {
      const n = await reindexSearch(model.apiId);
      toast.success(`${model.apiId}: ${n} entr${n === 1 ? "y" : "ies"} reindexed.`);
    } catch (e) {
      setError(describeError(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <section className="card bg-base-100 border-base-300 border" aria-label={`Search settings of ${model.apiId}`}>
      <div className="card-body gap-3 p-4">
        <div className="flex flex-wrap items-center gap-3">
          <label className="flex cursor-pointer items-center gap-3">
            <input
              type="checkbox"
              className="toggle toggle-primary"
              checked={enabled}
              disabled={searchableFields.length === 0}
              onChange={(e) => toggleEnabled(e.target.checked)}
            />
            <h2 className="card-title text-lg">{model.apiId}</h2>
          </label>
          {model.active && <span className="badge badge-success badge-sm">searchable</span>}
          {searchableFields.length === 0 && (
            <span className="text-base-content/60 text-sm">No TEXT or MARKDOWN field: it cannot be searched.</span>
          )}
          <div className="ml-auto flex gap-2">
            {model.active && (
              <button type="button" className="btn btn-ghost btn-sm" onClick={() => void reindex()} disabled={!!busy}>
                <ButtonSpinner busy={busy === "reindex"} />
                Reindex
              </button>
            )}
            <button
              type="button"
              className="btn btn-primary btn-sm"
              onClick={() => void save()}
              disabled={!dirty || !!busy || (enabled && !hasText)}
            >
              <ButtonSpinner busy={busy === "save"} />
              Save
            </button>
          </div>
        </div>

        {enabled && (
          <div className="overflow-x-auto">
            <table className="table-sm table">
              <thead>
                <tr>
                  <th className="w-8">
                    <span className="sr-only">Indexed</span>
                  </th>
                  <th>Field</th>
                  <th>Type</th>
                  <th>Indexed as</th>
                  {engine === "typesense" && <th>Typesense field</th>}
                </tr>
              </thead>
              <tbody>
                {model.fields.map((f) => (
                  <tr key={f.name} className={f.supported ? "" : "opacity-50"}>
                    <td>
                      <input
                        type="checkbox"
                        className="checkbox checkbox-sm"
                        aria-label={`Index ${f.name}`}
                        checked={selected.has(f.name)}
                        disabled={!f.supported}
                        onChange={(e) => toggleField(f.name, e.target.checked)}
                      />
                    </td>
                    <td className="font-mono text-sm">{f.name}</td>
                    <td>
                      <span className="badge badge-ghost badge-sm">{f.type}</span>
                    </td>
                    <td className="text-sm">
                      {f.indexType ? (
                        <>
                          {INDEX_LABELS[f.indexType]}
                          {f.searchable && <span className="badge badge-info badge-xs ml-2">searched</span>}
                        </>
                      ) : (
                        <span className="text-base-content/60">Not indexable</span>
                      )}
                    </td>
                    {engine === "typesense" && (
                      <td>
                        {f.engineConfig && <code className="text-xs break-all">{JSON.stringify(f.engineConfig)}</code>}
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {enabled && !hasText && (
          <p className="text-warning text-sm">Select at least one TEXT or MARKDOWN field to search.</p>
        )}
        {error && (
          <div role="alert" className="alert alert-error text-sm">
            {error}
          </div>
        )}
      </div>
    </section>
  );
}
