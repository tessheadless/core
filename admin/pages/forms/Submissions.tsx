// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useEffect, useState } from "react";
import { Modal } from "../../components/Modal";
import { useToast } from "../../components/Toasts";
import { formatDate } from "../../lib/format";
import {
  deleteSubmissions,
  fetchSubmissions,
  submissionsCsv,
  valueText,
  type Form,
  type FormSubmission,
} from "../../lib/forms";
import { describeError } from "../../lib/graphql";

const PAGE = 50;

function show(v: unknown): string {
  if (v === undefined || v === null || v === "") return "—";
  if (typeof v === "boolean") return v ? "Yes" : "No";
  return Array.isArray(v) ? v.join(", ") : valueText(v);
}

/** Submissions of a form, newest first, with CSV export and deletion. */
export function Submissions({
  form,
  onClose,
  onChanged,
}: {
  form: Form;
  onClose: () => void;
  onChanged: (count: number) => void;
}) {
  const toast = useToast();
  const [page, setPage] = useState(0);
  const [version, setVersion] = useState(0);
  const [result, setResult] = useState<{ total: number; items: FormSubmission[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    fetchSubmissions(form.id, PAGE, page * PAGE).then(
      (r) => !cancelled && (setResult(r), setError(null)),
      (e: unknown) => !cancelled && setError(describeError(e)),
    );
    return () => {
      cancelled = true;
    };
  }, [form.id, page, version]);

  async function exportCsv() {
    setBusy(true);
    try {
      const all: FormSubmission[] = [];
      for (let skip = 0; ; skip += 500) {
        const r = await fetchSubmissions(form.id, 500, skip);
        all.push(...r.items);
        if (all.length >= r.total || r.items.length === 0) break;
      }
      const url = URL.createObjectURL(new Blob([submissionsCsv(form, all)], { type: "text/csv" }));
      const a = document.createElement("a");
      a.href = url;
      a.download = `${form.slug}-submissions.csv`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      toast.error(describeError(e));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    setBusy(true);
    try {
      const n = await deleteSubmissions(form.id, [...selected]);
      toast.success(`${n} submission${n === 1 ? "" : "s"} deleted.`);
      setSelected(new Set());
      onChanged((result?.total ?? n) - n);
      setVersion((v) => v + 1);
    } catch (e) {
      toast.error(describeError(e));
    } finally {
      setBusy(false);
    }
  }

  const pages = result ? Math.max(1, Math.ceil(result.total / PAGE)) : 1;
  return (
    <Modal title={`Submissions: ${form.title}`} onClose={onClose} variant="drawer">
      {() => (
        <div className="mt-3">
          <div className="mb-3 flex flex-wrap items-center gap-2">
            <span className="text-base-content/70 text-sm">
              {result ? `${result.total} submission${result.total === 1 ? "" : "s"}` : ""}
            </span>
            <div className="ml-auto flex gap-2">
              {selected.size > 0 && (
                <button
                  type="button"
                  className="btn btn-sm btn-error btn-outline"
                  disabled={busy}
                  onClick={() => void remove()}
                >
                  Delete {selected.size}
                </button>
              )}
              <button
                type="button"
                className="btn btn-sm"
                disabled={busy || !result?.total}
                onClick={() => void exportCsv()}
              >
                Export CSV
              </button>
            </div>
          </div>
          {error && (
            <div role="alert" className="alert alert-error mb-3 text-sm">
              {error}
            </div>
          )}
          {result?.items.length === 0 && <p className="text-base-content/60 py-8 text-center">No submission yet.</p>}
          <ul className="flex flex-col gap-3">
            {result?.items.map((s) => (
              <li key={s.id} className="border-base-300 rounded-box border p-3">
                <div className="mb-2 flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    className="checkbox checkbox-xs"
                    aria-label={`Select the submission of ${formatDate(s.submittedAt)}`}
                    checked={selected.has(s.id)}
                    onChange={(e) =>
                      setSelected((cur) => {
                        const next = new Set(cur);
                        if (e.target.checked) next.add(s.id);
                        else next.delete(s.id);
                        return next;
                      })
                    }
                  />
                  <span className="font-semibold">{formatDate(s.submittedAt)}</span>
                  {s.source && <span className="text-base-content/60 truncate text-xs">from {s.source}</span>}
                </div>
                <dl className="grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-x-3 gap-y-1 text-sm">
                  {form.fields.map((f) => (
                    <div key={f.name} className="contents">
                      <dt className="text-base-content/70">{f.label}</dt>
                      <dd className="break-words whitespace-pre-wrap">{show(s.data[f.name])}</dd>
                    </div>
                  ))}
                </dl>
              </li>
            ))}
          </ul>
          {pages > 1 && (
            <div className="join mt-4">
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
      )}
    </Modal>
  );
}
