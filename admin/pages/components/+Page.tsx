// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState } from "react";
import { ConfirmModal } from "../../components/ConfirmModal";
import { ModelEditor } from "../../components/ModelEditor";
import { Restricted } from "../../components/Restricted";
import { useToast } from "../../components/Toasts";
import { componentUsers, deleteComponent, fetchModels, type ComponentDefinition } from "../../lib/contentTypes";
import { useQuery } from "../../lib/useQuery";
import { useViewer } from "../../lib/viewer";

type Dialog =
  | { kind: "create" }
  | { kind: "edit"; component: ComponentDefinition }
  | { kind: "delete"; component: ComponentDefinition };

export default function Page() {
  const { isAdmin } = useViewer();
  return (
    <Restricted allowed={isAdmin} what="manage components">
      <Components />
    </Restricted>
  );
}

function Components() {
  const toast = useToast();
  const { data, error, loading, reload, setData } = useQuery(fetchModels);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const components = data ? [...data.components].sort((a, b) => a.apiId.localeCompare(b.apiId)) : undefined;

  function saved(c: ComponentDefinition, created: boolean) {
    setData((d) => ({
      ...d,
      components: created ? [...d.components, c] : d.components.map((x) => (x.apiId === c.apiId ? c : x)),
    }));
    toast.success(created ? `Component ${c.apiId} created.` : `Component ${c.apiId} updated everywhere it is used.`);
  }

  return (
    <>
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-3xl font-bold">Components</h1>
        <div className="flex gap-2">
          <button type="button" className="btn btn-ghost btn-sm" onClick={reload} disabled={loading}>
            {loading && <span className="loading loading-spinner loading-xs" aria-hidden="true" />}
            Refresh
          </button>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={() => setDialog({ kind: "create" })}
            disabled={!data}
          >
            + New component
          </button>
        </div>
      </div>
      <p className="text-base-content/70 mb-6">
        Components are reusable sets of fields (an SEO block, a call-to-action, a hero banner…). Add them to content
        models with a <strong>Component</strong> field, offer several in a <strong>Dynamic zone</strong> to build pages
        from blocks, or nest them in other components.
      </p>

      {error && (
        <div role="alert" className="alert alert-error mb-4">
          <span>{error}</span>
          <button type="button" className="btn btn-sm" onClick={reload}>
            Retry
          </button>
        </div>
      )}
      {loading && !data && <span className="loading loading-spinner loading-lg" aria-label="Loading" />}
      {components?.length === 0 && <p className="text-base-content/70">No components yet.</p>}
      {data && components && components.length > 0 && (
        <div className="border-base-300 rounded-box overflow-x-auto border">
          <table className="table">
            <caption className="sr-only">Components</caption>
            <thead>
              <tr>
                <th scope="col">API ID</th>
                <th scope="col">Fields</th>
                <th scope="col">Used by</th>
                <th scope="col">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {components.map((c) => {
                const users = componentUsers(c.apiId, data.contentTypes, data.components);
                return (
                  <tr key={c.apiId} className="align-top">
                    <td>
                      <div className="font-mono font-semibold">{c.apiId}</div>
                      {c.description && <div className="text-base-content/70 max-w-xs text-sm">{c.description}</div>}
                    </td>
                    <td>
                      <div className="flex flex-wrap gap-1">
                        {c.fields.map((f) => (
                          <span key={f.name} className="badge badge-ghost badge-sm font-mono">
                            {f.name}: {f.type === "COMPONENT" ? f.component : f.type}
                            {f.many || f.repeatable ? "[]" : ""}
                            {f.required ? "!" : ""}
                          </span>
                        ))}
                      </div>
                    </td>
                    <td className="text-sm">
                      {users.length ? (
                        users.map((u) => (
                          <code key={u} className="mr-1">
                            {u}
                          </code>
                        ))
                      ) : (
                        <span className="text-base-content/50">—</span>
                      )}
                    </td>
                    <td className="text-right whitespace-nowrap">
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs"
                        onClick={() => setDialog({ kind: "edit", component: c })}
                      >
                        Edit<span className="sr-only"> {c.apiId}</span>
                      </button>
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs text-error"
                        onClick={() => setDialog({ kind: "delete", component: c })}
                      >
                        Delete<span className="sr-only"> {c.apiId}</span>
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {data && (dialog?.kind === "create" || dialog?.kind === "edit") && (
        <ModelEditor
          kind="component"
          model={dialog.kind === "edit" ? dialog.component : null}
          types={data.contentTypes}
          components={data.components}
          onClose={() => setDialog(null)}
          onSaved={(m, created) => saved(m as ComponentDefinition, created)}
        />
      )}
      {data && dialog?.kind === "delete" && (
        <ConfirmModal
          title={`Delete ${dialog.component.apiId}?`}
          confirmLabel="Delete component"
          onClose={() => setDialog(null)}
          onConfirm={async () => {
            const users = componentUsers(dialog.component.apiId, data.contentTypes, data.components);
            if (users.length) throw new Error(`Still used by ${users.join(", ")}: remove those fields first.`);
            await deleteComponent(dialog.component.apiId);
            setData((d) => ({ ...d, components: d.components.filter((c) => c.apiId !== dialog.component.apiId) }));
            toast.success(`Component ${dialog.component.apiId} deleted.`);
          }}
        >
          The component definition is deleted. Only possible when no content model or component uses it anymore.
        </ConfirmModal>
      )}
    </>
  );
}
