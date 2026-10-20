// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useEffect, useId, useState } from "react";
import { ButtonSpinner } from "../../components/Field";
import { Modal } from "../../components/Modal";
import { useToast } from "../../components/Toasts";
import { ApiError, describeError } from "../../lib/graphql";
import { formatDate } from "../../lib/format";
import {
  approveEntry,
  displayValue,
  fetchEntryDetail,
  requestChanges,
  type EntryDetail,
  type FieldValues,
  type HistoryAction,
  type QueueEntry,
  type WorkflowType,
} from "../../lib/workflow";

interface EntryDrawerProps {
  type: WorkflowType;
  entry: QueueEntry;
  canAct: boolean;
  onClose: () => void;
  /** Called after a successful approve / request changes. */
  onDone: () => void;
}

type Detail = { draft: EntryDetail; published: FieldValues | null };

function errorText(e: unknown): string {
  if (e instanceof ApiError && (e.code === "ID_EXPIRED" || e.code === "NOT_FOUND"))
    return "This entry changed or its link expired. Close this panel and refresh the queue.";
  return describeError(e);
}

export function EntryDrawer({ type, entry, canAct, onClose, onDone }: EntryDrawerProps) {
  const [detail, setDetail] = useState<Detail | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetchEntryDetail(type, entry.id, entry.publishedAt !== null).then(
      (result) => {
        if (cancelled) return;
        if (result.draft) setDetail({ draft: result.draft, published: result.published });
        else setLoadError("This entry no longer exists.");
      },
      (e: unknown) => {
        if (!cancelled) setLoadError(errorText(e));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [type, entry]);

  return (
    <Modal title={entry.title || `${type.typeName} ${entry.ref}`} onClose={onClose} variant="drawer">
      {(close) => (
        <div className="mt-2 flex flex-col gap-6">
          {loadError && (
            <div role="alert" className="alert alert-error text-sm">
              {loadError}
            </div>
          )}
          {!detail && !loadError && (
            <div className="flex justify-center py-12">
              <span className="loading loading-spinner loading-lg" aria-label="Loading entry" />
            </div>
          )}
          {detail && (
            <>
              <Meta type={type} draft={detail.draft} />
              <Comparison type={type} draft={detail.draft.values} published={detail.published} />
              {detail.draft.workflowStatus === "IN_REVIEW" &&
                (canAct ? (
                  <ReviewActions
                    type={type}
                    id={detail.draft.id}
                    onDone={() => {
                      close();
                      onDone();
                    }}
                  />
                ) : (
                  <p className="text-base-content/70 text-sm">You can view this entry but not review it.</p>
                ))}
              <Timeline events={detail.draft.history ?? []} />
            </>
          )}
        </div>
      )}
    </Modal>
  );
}

const STATUS_LABEL: Record<EntryDetail["workflowStatus"], string> = {
  DRAFT: "Draft",
  IN_REVIEW: "In review",
  CHANGES_REQUESTED: "Changes requested",
  PUBLISHED: "Published",
};

function Meta({ type, draft }: { type: WorkflowType; draft: EntryDetail }) {
  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
      <dt className="text-base-content/70">Type</dt>
      <dd className="font-mono">{type.typeName}</dd>
      <dt className="text-base-content/70">Status</dt>
      <dd>
        <span className="badge badge-sm badge-outline">{STATUS_LABEL[draft.workflowStatus]}</span>
      </dd>
      <dt className="text-base-content/70">Reference</dt>
      <dd className="font-mono break-all">{draft.ref}</dd>
      <dt className="text-base-content/70">Created</dt>
      <dd>
        {formatDate(draft.createdAt)}
        {draft.createdBy && ` by ${draft.createdBy}`}
      </dd>
      <dt className="text-base-content/70">Updated</dt>
      <dd>
        {formatDate(draft.updatedAt)}
        {draft.updatedBy && ` by ${draft.updatedBy}`}
      </dd>
      <dt className="text-base-content/70">Last published</dt>
      <dd>{draft.publishedAt ? formatDate(draft.publishedAt) : "Never"}</dd>
    </dl>
  );
}

function Value({ value }: { value: unknown }) {
  const text = displayValue(value);
  if (!text) return <span className="text-base-content/50 italic">empty</span>;
  return <pre className="font-sans text-sm break-words whitespace-pre-wrap">{text}</pre>;
}

function Comparison({
  type,
  draft,
  published,
}: {
  type: WorkflowType;
  draft: FieldValues;
  published: FieldValues | null;
}) {
  if (type.fields.length === 0)
    return <p className="text-base-content/70 text-sm">This type has no simple fields to show.</p>;
  const changedCount = published
    ? type.fields.filter((f) => displayValue(draft[f.name]) !== displayValue(published[f.name])).length
    : 0;
  return (
    <section aria-labelledby="entry-fields">
      <h3 id="entry-fields" className="mb-2 font-semibold">
        Content
        {published && (
          <span className="text-base-content/70 ml-2 text-sm font-normal">
            {changedCount === 0
              ? "no change from the published version"
              : `${changedCount} changed from the published version`}
          </span>
        )}
      </h3>
      <div className="border-base-300 rounded-box overflow-x-auto border">
        <table className="table-sm table">
          <thead>
            <tr>
              <th scope="col">Field</th>
              <th scope="col">{published ? "Draft (proposed)" : "Draft"}</th>
              {published && <th scope="col">Published</th>}
            </tr>
          </thead>
          <tbody>
            {type.fields.map((f) => {
              const changed = published !== null && displayValue(draft[f.name]) !== displayValue(published[f.name]);
              return (
                <tr key={f.name} className={`align-top ${changed ? "bg-warning/10" : ""}`}>
                  <th scope="row" className="font-mono text-xs font-semibold">
                    {f.name}
                    {changed && <span className="badge badge-warning badge-xs ml-1 font-sans">changed</span>}
                  </th>
                  <td className="min-w-40">
                    <Value value={draft[f.name]} />
                  </td>
                  {published && (
                    <td className="min-w-40">
                      <Value value={published[f.name]} />
                    </td>
                  )}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function ReviewActions({ type, id, onDone }: { type: WorkflowType; id: string; onDone: () => void }) {
  const toast = useToast();
  const commentId = useId();
  const hintId = useId();
  const [comment, setComment] = useState("");
  const [busy, setBusy] = useState<"approve" | "changes" | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function run(action: "approve" | "changes") {
    const text = comment.trim();
    if (action === "changes" && !text) {
      setError("Explain what needs to change: the comment is required to request changes.");
      return;
    }
    setBusy(action);
    setError(null);
    try {
      if (action === "approve") await approveEntry(type, id, text || null);
      else await requestChanges(type, id, text);
      toast.success(action === "approve" ? "Approved and published." : "Changes requested.");
      onDone();
    } catch (e) {
      setError(errorText(e));
      setBusy(null);
    }
  }

  return (
    <section aria-labelledby="review-title" className="bg-base-200 rounded-box p-4">
      <h3 id="review-title" className="mb-2 font-semibold">
        Your review
      </h3>
      <fieldset className="fieldset">
        <label className="fieldset-label" htmlFor={commentId}>
          Comment
        </label>
        <textarea
          id={commentId}
          className={`textarea w-full ${error ? "textarea-error" : ""}`}
          rows={3}
          value={comment}
          onChange={(e) => setComment(e.target.value)}
          disabled={busy !== null}
          aria-describedby={hintId}
          aria-invalid={error ? true : undefined}
        />
        <p id={hintId} className="text-base-content/70 text-xs">
          Optional when approving, required when requesting changes. The author sees it in the history.
        </p>
      </fieldset>
      {error && (
        <p role="alert" className="text-error mt-2 text-sm">
          {error}
        </p>
      )}
      <div className="mt-3 flex flex-wrap justify-end gap-2">
        <button type="button" className="btn btn-warning" disabled={busy !== null} onClick={() => void run("changes")}>
          <ButtonSpinner busy={busy === "changes"} />
          Request changes
        </button>
        <button type="button" className="btn btn-success" disabled={busy !== null} onClick={() => void run("approve")}>
          <ButtonSpinner busy={busy === "approve"} />
          Approve and publish
        </button>
      </div>
    </section>
  );
}

const ACTION_LABEL: Record<HistoryAction, string> = {
  CREATED: "Created",
  UPDATED: "Updated",
  SUBMITTED: "Submitted for review",
  CHANGES_REQUESTED: "Changes requested",
  APPROVED: "Approved",
  PUBLISHED: "Published",
  UNPUBLISHED: "Unpublished",
};

const ACTION_CLASS: Partial<Record<HistoryAction, string>> = {
  SUBMITTED: "badge-info",
  CHANGES_REQUESTED: "badge-warning",
  APPROVED: "badge-success",
  PUBLISHED: "badge-success",
  UNPUBLISHED: "badge-error",
};

function Timeline({ events }: { events: { action: HistoryAction; at: string; by: string; comment: string | null }[] }) {
  return (
    <section aria-labelledby="history-title">
      <h3 id="history-title" className="mb-2 font-semibold">
        History
      </h3>
      {events.length === 0 ? (
        <p className="text-base-content/60 text-sm">No history recorded.</p>
      ) : (
        <ol className="border-base-300 ml-2 border-l">
          {events.map((event, i) => (
            <li key={`${event.at}-${i}`} className="relative pb-4 pl-5">
              <span
                className="bg-base-content/40 absolute top-1.5 -left-[5px] size-2.5 rounded-full"
                aria-hidden="true"
              />
              <div className="flex flex-wrap items-center gap-2 text-sm">
                <span className={`badge badge-sm ${ACTION_CLASS[event.action] ?? "badge-ghost"}`}>
                  {ACTION_LABEL[event.action]}
                </span>
                <span>by {event.by}</span>
                <time dateTime={event.at} className="text-base-content/60 text-xs">
                  {formatDate(event.at)}
                </time>
              </div>
              {event.comment && (
                <blockquote className="bg-base-200 rounded-box mt-1 p-2 text-sm whitespace-pre-wrap">
                  {event.comment}
                </blockquote>
              )}
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
