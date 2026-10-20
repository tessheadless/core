// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState } from "react";
import { Restricted } from "../../components/Restricted";
import { useToast } from "../../components/Toasts";
import { ModelEditor } from "../../components/ModelEditor";
import { BUILTIN_TYPES, fetchModels, setWorkflow, type ContentType } from "../../lib/contentTypes";
import { describeError } from "../../lib/graphql";
import { useQuery } from "../../lib/useQuery";
import { useViewer } from "../../lib/viewer";
import { DeleteModelModal } from "./DeleteModelModal";

type Dialog = { kind: "create" } | { kind: "edit"; type: ContentType } | { kind: "delete"; type: ContentType };

export default function Page() {
  const { isAdmin } = useViewer();
  return (
    <Restricted allowed={isAdmin} what="manage content models">
      <ContentModels />
    </Restricted>
  );
}

function ContentModels() {
  const toast = useToast();
  const { data, error, loading, reload, setData } = useQuery(fetchModels);
  const [pending, setPending] = useState<ReadonlySet<string>>(new Set());
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const types = data?.contentTypes;

  function saved(type: ContentType, created: boolean) {
    setData((d) => ({
      ...d,
      contentTypes: created
        ? [...d.contentTypes, type].sort((a, b) => a.apiId.localeCompare(b.apiId))
        : d.contentTypes.map((t) => (t.apiId === type.apiId ? type : t)),
    }));
    toast.success(
      created
        ? `${type.apiId} created: its queries and mutations are now available in the API.`
        : `${type.apiId} updated.`,
    );
  }

  function deleted(apiId: string) {
    setData((d) => ({ ...d, contentTypes: d.contentTypes.filter((t) => t.apiId !== apiId) }));
    toast.success(`${apiId} and its entries were deleted.`);
  }

  /** Content types with a relation to `apiId`. */
  function referencedBy(apiId: string): string[] {
    return (types ?? [])
      .filter((t) => t.apiId !== apiId && t.fields.some((f) => f.type === "RELATION" && f.relationTarget === apiId))
      .map((t) => t.apiId);
  }

  async function toggleWorkflow(type: ContentType, workflow: boolean) {
    setPending((s) => new Set(s).add(type.apiId));
    try {
      const updated = await setWorkflow(type.apiId, workflow);
      setData((d) => ({ ...d, contentTypes: d.contentTypes.map((t) => (t.apiId === updated.apiId ? updated : t)) }));
      toast.success(
        updated.workflow
          ? `${type.apiId}: entries now go through review before publishing.`
          : `${type.apiId}: review workflow disabled; contributors can publish directly.`,
      );
    } catch (e) {
      toast.error(`Could not update ${type.apiId}: ${describeError(e)}`);
    } finally {
      setPending((s) => {
        const next = new Set(s);
        next.delete(type.apiId);
        return next;
      });
    }
  }

  return (
    <>
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-3xl font-bold">Content models</h1>
        <div className="flex gap-2">
          <button type="button" className="btn btn-ghost btn-sm" onClick={reload} disabled={loading}>
            {loading && <span className="loading loading-spinner loading-xs" aria-hidden="true" />}
            Refresh
          </button>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={() => setDialog({ kind: "create" })}
            disabled={!types}
          >
            + New content model
          </button>
        </div>
      </div>
      <p className="text-base-content/70 mb-6">
        The content types defined on this CMS. Each one gets its own GraphQL queries and mutations as soon as it is
        saved. With the review workflow on, contributors submit drafts for review and a reviewer or admin approves them
        before they are published. <code>Asset</code> is the built-in media library: reference it from your models with
        a relation field.
      </p>

      {error && (
        <div role="alert" className="alert alert-error mb-4">
          <span>{error}</span>
          <button type="button" className="btn btn-sm" onClick={reload}>
            Retry
          </button>
        </div>
      )}
      {loading && !types && <span className="loading loading-spinner loading-lg" aria-label="Loading" />}
      {types?.length === 0 && <p className="text-base-content/70">No content types defined yet.</p>}
      {data && dialog?.kind === "create" && (
        <ModelEditor
          kind="contentType"
          model={null}
          types={data.contentTypes}
          components={data.components}
          onClose={() => setDialog(null)}
          onSaved={(m, created) => saved(m as ContentType, created)}
        />
      )}
      {data && dialog?.kind === "edit" && (
        <ModelEditor
          kind="contentType"
          model={dialog.type}
          types={data.contentTypes}
          components={data.components}
          readOnly={BUILTIN_TYPES.includes(dialog.type.apiId)}
          onClose={() => setDialog(null)}
          onSaved={(m, created) => saved(m as ContentType, created)}
        />
      )}
      {dialog?.kind === "delete" && (
        <DeleteModelModal
          apiId={dialog.type.apiId}
          referencedBy={referencedBy(dialog.type.apiId)}
          onClose={() => setDialog(null)}
          onDeleted={deleted}
        />
      )}
      {types && types.length > 0 && (
        <div className="border-base-300 rounded-box overflow-x-auto border">
          <table className="table">
            <caption className="sr-only">Content types</caption>
            <thead>
              <tr>
                <th scope="col">API ID</th>
                <th scope="col">Plural API ID</th>
                <th scope="col">Review workflow</th>
                <th scope="col">Fields</th>
                <th scope="col">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {types.map((t) => {
                const builtin = BUILTIN_TYPES.includes(t.apiId);
                return (
                  <tr key={t.apiId} className="align-top">
                    <td>
                      <div className="font-mono font-semibold">
                        {t.apiId}
                        {builtin && <span className="badge badge-neutral badge-sm ml-2 font-sans">Built-in</span>}
                      </div>
                      {t.description && <div className="text-base-content/70 max-w-xs text-sm">{t.description}</div>}
                    </td>
                    <td className="font-mono">{t.pluralApiId}</td>
                    <td>
                      <label className="label cursor-pointer gap-2">
                        <input
                          type="checkbox"
                          className="toggle toggle-success toggle-sm"
                          checked={t.workflow}
                          disabled={builtin || pending.has(t.apiId)}
                          onChange={(e) => void toggleWorkflow(t, e.target.checked)}
                          aria-label={`Review workflow for ${t.apiId}`}
                        />
                        <span className="text-sm">{t.workflow ? "On" : "Off"}</span>
                        {pending.has(t.apiId) && (
                          <span className="loading loading-spinner loading-xs" aria-label="Saving" />
                        )}
                      </label>
                    </td>
                    <td>
                      <div className="flex flex-wrap gap-1">
                        {t.fields.map((f) => (
                          <span
                            key={f.name}
                            className="badge badge-ghost badge-sm font-mono"
                            title={f.description ?? undefined}
                          >
                            {f.name}:{" "}
                            {f.type === "COMPONENT" ? f.component : f.type === "DYNAMIC_ZONE" ? "zone" : f.type}
                            {f.many || f.repeatable ? "[]" : ""}
                            {f.required ? "!" : ""}
                          </span>
                        ))}
                      </div>
                    </td>
                    <td className="text-right whitespace-nowrap">
                      {builtin ? (
                        <button
                          type="button"
                          className="btn btn-ghost btn-xs"
                          onClick={() => setDialog({ kind: "edit", type: t })}
                        >
                          View<span className="sr-only"> {t.apiId}</span>
                        </button>
                      ) : (
                        <>
                          <button
                            type="button"
                            className="btn btn-ghost btn-xs"
                            onClick={() => setDialog({ kind: "edit", type: t })}
                          >
                            Edit<span className="sr-only"> {t.apiId}</span>
                          </button>
                          <button
                            type="button"
                            className="btn btn-ghost btn-xs text-error"
                            onClick={() => setDialog({ kind: "delete", type: t })}
                          >
                            Delete<span className="sr-only"> {t.apiId}</span>
                          </button>
                        </>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
