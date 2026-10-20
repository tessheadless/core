// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState } from "react";
import { ConfirmModal } from "../../components/ConfirmModal";
import { Modal } from "../../components/Modal";
import { Restricted } from "../../components/Restricted";
import { useToast } from "../../components/Toasts";
import { deleteForm, fetchForms, type Form } from "../../lib/forms";
import { useQuery } from "../../lib/useQuery";
import { useViewer } from "../../lib/viewer";
import { FormEditor } from "./FormEditor";
import { Submissions } from "./Submissions";

export default function Page() {
  const { isAdmin } = useViewer();
  return (
    <Restricted allowed={isAdmin} what="manage forms">
      <Forms />
    </Restricted>
  );
}

function Forms() {
  const toast = useToast();
  const { data, error, loading, reload, setData } = useQuery(fetchForms);
  const [editing, setEditing] = useState<Form | "new" | null>(null);
  const [removing, setRemoving] = useState<Form | null>(null);
  const [viewing, setViewing] = useState<Form | null>(null);
  const [using, setUsing] = useState<Form | null>(null);

  const upsert = (f: Form) =>
    setData((d) => (d.some((x) => x.id === f.id) ? d.map((x) => (x.id === f.id ? f : x)) : [...d, f]));

  return (
    <>
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-3xl font-bold">Forms</h1>
        <div className="flex gap-2">
          <button type="button" className="btn btn-ghost btn-sm" onClick={reload} disabled={loading}>
            Refresh
          </button>
          <button type="button" className="btn btn-primary btn-sm" onClick={() => setEditing("new")}>
            New form
          </button>
        </div>
      </div>
      <p className="text-base-content/70 mb-6 max-w-3xl text-sm">
        Contact, reservation or sign-up forms for your front-ends. They read the definition at{" "}
        <code>/api/forms/&lt;slug&gt;</code> and post submissions there; Tess validates and keeps them, and sends a{" "}
        <code>form.submitted</code> event to your webhooks and n8n workflows.
      </p>

      {error && (
        <div role="alert" className="alert alert-error mb-4">
          <span>Could not load forms: {error}</span>
        </div>
      )}
      {data && data.length === 0 && <p className="text-base-content/60 py-8 text-center">No form yet.</p>}
      {data && data.length > 0 && (
        <div className="overflow-x-auto">
          <table className="table">
            <thead>
              <tr>
                <th>Form</th>
                <th>Fields</th>
                <th>Submissions</th>
                <th>Open</th>
                <th className="sr-only">Actions</th>
              </tr>
            </thead>
            <tbody>
              {data.map((f) => (
                <tr key={f.id}>
                  <td>
                    <div className="font-semibold">{f.title}</div>
                    <div className="text-base-content/60 font-mono text-xs">/api/forms/{f.slug}</div>
                  </td>
                  <td>{f.fields.length}</td>
                  <td>
                    <button type="button" className="link" onClick={() => setViewing(f)}>
                      {f.submissionCount}
                    </button>
                  </td>
                  <td>
                    {f.enabled ? (
                      <span className="badge badge-success badge-sm">open</span>
                    ) : (
                      <span className="badge badge-ghost badge-sm">closed</span>
                    )}
                  </td>
                  {/* Every action is a labelled button; the linter flags the cell itself. */}
                  {/* oxlint-disable-next-line jsx-a11y/control-has-associated-label */}
                  <td>
                    <div className="flex justify-end gap-1">
                      <button type="button" className="btn btn-xs" onClick={() => setViewing(f)}>
                        Submissions
                      </button>
                      <button type="button" className="btn btn-xs btn-ghost" onClick={() => setUsing(f)}>
                        Use it
                      </button>
                      <button type="button" className="btn btn-xs btn-ghost" onClick={() => setEditing(f)}>
                        Edit
                      </button>
                      <button type="button" className="btn btn-xs btn-ghost text-error" onClick={() => setRemoving(f)}>
                        Delete
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {editing && (
        <FormEditor
          form={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={(f) => {
            upsert(f);
            toast.success(`Form “${f.title}” saved.`);
          }}
        />
      )}
      {viewing && (
        <Submissions
          form={viewing}
          onClose={() => setViewing(null)}
          onChanged={(count) => upsert({ ...viewing, submissionCount: count })}
        />
      )}
      {using && <UseIt form={using} onClose={() => setUsing(null)} />}
      {removing && (
        <ConfirmModal
          title={`Delete “${removing.title}”?`}
          confirmLabel="Delete form"
          onClose={() => setRemoving(null)}
          onConfirm={async () => {
            await deleteForm(removing.id);
            setData((d) => d.filter((x) => x.id !== removing.id));
          }}
        >
          Its {removing.submissionCount} submission{removing.submissionCount === 1 ? "" : "s"} are deleted too.
          Front-ends posting to <code>/api/forms/{removing.slug}</code> get a 404.
        </ConfirmModal>
      )}
    </>
  );
}

/** How a front-end uses the form. */
function UseIt({ form, onClose }: { form: Form; onClose: () => void }) {
  const endpoint = `${window.location.origin}/api/forms/${form.slug}`;
  const sample = Object.fromEntries(
    form.fields.map((f) => [f.name, f.type === "CHECKBOX" ? true : f.type === "MULTI_SELECT" ? [] : "…"]),
  );
  const js = `await fetch("${endpoint}", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify(${JSON.stringify(sample, null, 2).replace(/\n/g, "\n  ")}),
});`;
  const html = `<form method="post" action="${endpoint}">
${form.fields.map((f) => `  <input name="${f.name}"${f.required ? " required" : ""}>`).join("\n")}
  <input name="_hp" hidden tabindex="-1" autocomplete="off">
  <button>Send</button>
</form>`;
  return (
    <Modal title={`Use “${form.title}”`} onClose={onClose} wide>
      {(close) => (
        <div className="prose prose-sm mt-2 max-w-none">
          <p>
            Read the definition (fields, labels, options) with <code>GET {endpoint}</code>, render the form, then post
            it. Answers: <code>201</code> with the success message, <code>400</code> with an error per field,{" "}
            <code>429</code> when a visitor posts too often.
          </p>
          <pre>
            <code>{js}</code>
          </pre>
          <p>
            Or a plain HTML form{form.redirectUrl ? `, sent back to ${form.redirectUrl} after posting` : ""}. Keep the
            hidden <code>_hp</code> field empty: bots fill it, and Tess drops their submissions.
          </p>
          <pre>
            <code>{html}</code>
          </pre>
          <div className="modal-action">
            <button type="button" className="btn btn-primary" onClick={close}>
              Done
            </button>
          </div>
        </div>
      )}
    </Modal>
  );
}
