// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useEffect, useState } from "react";
import { ConfirmModal } from "../../components/ConfirmModal";
import { ButtonSpinner } from "../../components/Field";
import { Modal } from "../../components/Modal";
import { useToast } from "../../components/Toasts";
import type { ContentType } from "../../lib/contentTypes";
import {
  emptyForm,
  entryAction,
  FormError,
  loadEntry,
  resetRelationOptions,
  saveEntry,
  toInput,
  type EntryAction,
  type FormValues,
  type Models,
  type WorkflowStatus,
} from "../../lib/content";
import { ApiError, describeError } from "../../lib/graphql";
import { useViewer } from "../../lib/viewer";
import { FieldInput } from "./FieldInput";
import { StatusBadge } from "./StatusBadge";

interface EntryEditorProps {
  type: ContentType;
  models: Models;
  /** null: create a new entry. */
  entryId: string | null;
  onClose: () => void;
  /** Called after any change, so the list refreshes. */
  onChanged: () => void;
}

interface Loaded {
  id: string | null;
  workflowStatus: WorkflowStatus | null;
  publishedAt: string | null;
}

type Busy = "save" | "savePublish" | "saveSubmit" | "publish" | "unpublish" | null;

function errorText(e: unknown): string {
  if (e instanceof ApiError && e.code === "ID_EXPIRED")
    return "This link expired: close this panel and reopen the entry.";
  return describeError(e);
}

export function EntryEditor({ type, models, entryId, onClose, onChanged }: EntryEditorProps) {
  const { content } = useViewer();
  const toast = useToast();
  const [meta, setMeta] = useState<Loaded | null>(
    entryId ? null : { id: null, workflowStatus: null, publishedAt: null },
  );
  const [values, setValues] = useState<FormValues | null>(entryId ? null : emptyForm(type));
  const [loadError, setLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<Busy>(null);
  const [dirty, setDirty] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  useEffect(() => {
    resetRelationOptions();
    if (!entryId) return;
    let cancelled = false;
    loadEntry(type, entryId, models).then(
      (e) => {
        if (cancelled) return;
        setMeta({ id: e.id, workflowStatus: e.workflowStatus, publishedAt: e.publishedAt });
        setValues(e.values);
      },
      (e: unknown) => !cancelled && setLoadError(errorText(e)),
    );
    return () => {
      cancelled = true;
    };
  }, [type, entryId, models]);

  const existing = meta?.id != null;
  const canEdit = content.canEdit(type.apiId);
  const canPublish = content.canPublish(type.apiId, type.workflow);
  const canSubmit = content.canSubmit(type.apiId, type.workflow);
  const canDelete = existing && content.canDelete(type.apiId, meta?.publishedAt != null);
  const status = meta?.workflowStatus ?? null;
  const submittable = status === null || status === "DRAFT" || status === "CHANGES_REQUESTED";
  const readOnly = !canEdit;

  /** Saves the form, then optionally runs a workflow action on the saved entry. */
  async function run(kind: Exclude<Busy, null>, close: () => void) {
    if (!values || !meta) return;
    setBusy(kind);
    setError(null);
    try {
      let id = meta.id;
      if (kind === "save" || kind === "savePublish" || kind === "saveSubmit") {
        id = await saveEntry(type, id, toInput(type.fields, values, models));
      }
      const action: EntryAction | null =
        kind === "savePublish" || kind === "publish"
          ? "publish"
          : kind === "saveSubmit"
            ? "submit"
            : kind === "unpublish"
              ? "unpublish"
              : null;
      if (action && id) await entryAction(type, id, action);
      onChanged();
      toast.success(
        {
          save: existing ? "Draft saved." : "Entry created.",
          savePublish: "Saved and published.",
          saveSubmit: "Saved and submitted for review.",
          publish: "Published.",
          unpublish: "Unpublished.",
        }[kind],
      );
      close();
    } catch (e) {
      // Shown inline: toasts sit below the dialog's top layer.
      setError(e instanceof FormError ? e.message : errorText(e));
      setBusy(null);
      // A created entry exists even when the following action failed.
      if (!meta.id && !(e instanceof FormError)) onChanged();
    }
  }

  const title = existing ? `Edit ${type.apiId}` : `New ${type.apiId}`;

  return (
    <>
      <Modal title={title} onClose={onClose} variant="drawer">
        {(close) => (
          <form
            className="mt-2 flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              if (canEdit) void run("save", close);
            }}
          >
            {loadError && (
              <div role="alert" className="alert alert-error text-sm">
                {loadError}
              </div>
            )}
            {!values && !loadError && (
              <div className="flex justify-center py-12">
                <span className="loading loading-spinner loading-lg" aria-label="Loading entry" />
              </div>
            )}
            {values && meta && (
              <>
                {existing && (
                  <div className="flex flex-wrap items-center gap-2 text-sm">
                    {status && <StatusBadge status={status} />}
                    {meta.publishedAt ? (
                      <span className="badge badge-success badge-outline">live version published</span>
                    ) : (
                      <span className="badge badge-ghost">not published</span>
                    )}
                  </div>
                )}
                {readOnly && (
                  <output className="alert alert-info text-sm">
                    Read only: your role does not allow editing {type.apiId} entries.
                  </output>
                )}
                <div className="flex flex-col gap-1">
                  {type.fields.map((f) => (
                    <FieldInput
                      key={f.name}
                      field={f}
                      value={values[f.name]}
                      onChange={(v) => {
                        setDirty(true);
                        setValues((old) => ({ ...old, [f.name]: v }));
                      }}
                      models={models}
                      readOnly={readOnly}
                      existing={existing}
                    />
                  ))}
                </div>
                {error && (
                  <div role="alert" className="alert alert-error text-sm">
                    {error}
                  </div>
                )}
                <div className="bg-base-100 border-base-300 sticky bottom-0 -mx-6 flex flex-wrap gap-2 border-t px-6 py-3">
                  {canEdit && (
                    <button type="submit" className="btn btn-primary" disabled={busy !== null}>
                      <ButtonSpinner busy={busy === "save"} />
                      {existing ? "Save draft" : "Create draft"}
                    </button>
                  )}
                  {canEdit && canPublish && (
                    <button
                      type="button"
                      className="btn btn-success"
                      disabled={busy !== null}
                      onClick={() => void run("savePublish", close)}
                    >
                      <ButtonSpinner busy={busy === "savePublish"} />
                      Save & publish
                    </button>
                  )}
                  {canEdit && canSubmit && submittable && (
                    <button
                      type="button"
                      className="btn btn-secondary"
                      disabled={busy !== null}
                      onClick={() => void run("saveSubmit", close)}
                    >
                      <ButtonSpinner busy={busy === "saveSubmit"} />
                      Save & submit for review
                    </button>
                  )}
                  {existing && canPublish && !dirty && status !== "PUBLISHED" && (
                    <button
                      type="button"
                      className="btn btn-outline btn-success"
                      disabled={busy !== null}
                      onClick={() => void run("publish", close)}
                    >
                      <ButtonSpinner busy={busy === "publish"} />
                      Publish
                    </button>
                  )}
                  {existing && canPublish && meta.publishedAt && (
                    <button
                      type="button"
                      className="btn btn-outline btn-warning"
                      disabled={busy !== null}
                      onClick={() => void run("unpublish", close)}
                    >
                      <ButtonSpinner busy={busy === "unpublish"} />
                      Unpublish
                    </button>
                  )}
                  <span className="flex-1" />
                  {canDelete && (
                    <button
                      type="button"
                      className="btn btn-ghost text-error"
                      disabled={busy !== null}
                      onClick={() => setConfirmDelete(true)}
                    >
                      Delete
                    </button>
                  )}
                  <button type="button" className="btn btn-ghost" onClick={close}>
                    {readOnly ? "Close" : "Cancel"}
                  </button>
                </div>
              </>
            )}
          </form>
        )}
      </Modal>
      {confirmDelete && meta?.id && (
        <ConfirmModal
          title={`Delete this ${type.apiId}?`}
          confirmLabel="Delete"
          onConfirm={async () => {
            await entryAction(type, meta.id!, "delete");
            toast.success("Entry deleted.");
            onChanged();
            onClose();
          }}
          onClose={() => setConfirmDelete(false)}
        >
          The draft{meta.publishedAt ? " and the published version" : ""} will be deleted permanently.
        </ConfirmModal>
      )}
    </>
  );
}
