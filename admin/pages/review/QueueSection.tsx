// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useEffect, useState } from "react";
import { describeError } from "../../lib/graphql";
import { formatDate } from "../../lib/format";
import {
  fetchQueue,
  QUEUE_PAGE_SIZE,
  type QueueEntry,
  type QueueList,
  type WorkflowStatus,
  type WorkflowType,
} from "../../lib/workflow";

interface QueueSectionProps {
  type: WorkflowType;
  status: WorkflowStatus;
  /** Bumped by the parent to force a reload (after an action, or "Refresh"). */
  version: number;
  onOpen: (entry: QueueEntry) => void;
}

type Result = { list: QueueList } | { error: string };

/** Entries of one workflow content type in the given status. */
export function QueueSection({ type, status, version, onOpen }: QueueSectionProps) {
  const [result, setResult] = useState<{ key: string; status: WorkflowStatus; value: Result } | null>(null);
  const [retry, setRetry] = useState(0);
  const headingId = `queue-${type.typeName}-${status}`;
  const key = `${type.typeName}:${status}:${version}:${retry}`;
  const loading = result?.key !== key;
  // While reloading the same tab, keep showing the previous rows instead of flashing a spinner.
  const state = result && result.status === status ? result.value : null;

  useEffect(() => {
    let cancelled = false;
    fetchQueue(type, status).then(
      (list) => {
        if (!cancelled) setResult({ key, status, value: { list } });
      },
      (e: unknown) => {
        if (!cancelled) setResult({ key, status, value: { error: describeError(e) } });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [type, status, key]);

  return (
    <section className="mb-8" aria-labelledby={headingId}>
      <h2 id={headingId} className="mb-3 flex items-center gap-2 text-xl font-semibold">
        <span className="font-mono">{type.typeName}</span>
        {state && "list" in state && <span className="badge badge-neutral badge-sm">{state.list.total}</span>}
        {loading && <span className="loading loading-spinner loading-xs" aria-label="Loading" />}
      </h2>
      {state && "error" in state && (
        <div role="alert" className="alert alert-error">
          <span>
            Could not load {type.typeName} entries: {state.error}
          </span>
          <button type="button" className="btn btn-sm" onClick={() => setRetry((n) => n + 1)}>
            Retry
          </button>
        </div>
      )}
      {state && "list" in state && state.list.items.length === 0 && (
        <p className="text-base-content/60 text-sm">Nothing here.</p>
      )}
      {state && "list" in state && state.list.items.length > 0 && (
        <div className="border-base-300 rounded-box overflow-x-auto border">
          <table className="table">
            <caption className="sr-only">
              {type.typeName} entries {status === "IN_REVIEW" ? "in review" : "with changes requested"}
            </caption>
            <thead>
              <tr>
                <th scope="col">Entry</th>
                <th scope="col">Created by</th>
                <th scope="col">Updated by</th>
                <th scope="col">Updated</th>
                <th scope="col">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {state.list.items.map((entry) => (
                <tr key={entry.ref}>
                  <td>
                    <div className="font-semibold">{entry.title || <span className="italic">Untitled</span>}</div>
                    <div className="text-base-content/60 font-mono text-xs">{entry.ref}</div>
                    {entry.publishedAt && (
                      <span className="badge badge-ghost badge-xs">update of a published entry</span>
                    )}
                  </td>
                  <td>{entry.createdBy ?? "—"}</td>
                  <td>{entry.updatedBy ?? "—"}</td>
                  <td className="whitespace-nowrap">{formatDate(entry.updatedAt)}</td>
                  <td className="text-right">
                    <button type="button" className="btn btn-sm" onClick={() => onOpen(entry)}>
                      {status === "IN_REVIEW" ? "Review" : "View"}
                      <span className="sr-only"> {entry.title || entry.ref}</span>
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {state.list.total > state.list.items.length && (
            <p className="text-base-content/60 p-3 text-sm">
              Showing the {QUEUE_PAGE_SIZE} most recently updated of {state.list.total} entries.
            </p>
          )}
        </div>
      )}
    </section>
  );
}
