// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState } from "react";
import { ConfirmModal } from "../../components/ConfirmModal";
import { ButtonSpinner } from "../../components/Field";
import { useToast } from "../../components/Toasts";
import { formatDate } from "../../lib/format";
import { describeError } from "../../lib/graphql";
import {
  deleteTarget,
  fetchTargets,
  FORMAT_LABELS,
  PROVIDERS,
  publish,
  publicFileUrl,
  type DeliveryTarget,
  type OverviewData,
} from "../../lib/i18n";
import { useQuery } from "../../lib/useQuery";
import { TargetModal } from "./TargetModal";

export function DeliveryTab({ overview }: { overview: OverviewData }) {
  const toast = useToast();
  const { data: targets, error, loading, reload, setData } = useQuery(fetchTargets);
  const [editing, setEditing] = useState<DeliveryTarget | "new" | null>(null);
  const [removing, setRemoving] = useState<DeliveryTarget | null>(null);
  const [publishing, setPublishing] = useState<string | null>(null);

  async function run(id?: string) {
    setPublishing(id ?? "*");
    try {
      const reports = await publish(id);
      for (const r of reports) {
        if (r.ok) toast.success(`${r.name}: ${r.files} files published.`);
        else toast.error(`${r.name}: ${r.error ?? "failed"}`);
      }
      if (reports.length === 0) toast.info("No enabled target.");
      reload();
    } catch (e) {
      toast.error(describeError(e));
    } finally {
      setPublishing(null);
    }
  }

  return (
    <section aria-label="Delivery targets">
      <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
        <p className="text-base-content/70 max-w-2xl text-sm">
          Publish exported files to object storage (Garage, AWS S3, Cloudflare R2, MinIO, Azure Blob) for your apps and
          CDN. Files land at <code>{"{prefix}/{format}/{file}"}</code> with a <code>manifest.json</code>.
        </p>
        <div className="flex gap-2">
          {targets && targets.some((t) => t.enabled) && (
            <button type="button" className="btn btn-sm" disabled={publishing !== null} onClick={() => void run()}>
              <ButtonSpinner busy={publishing === "*"} />
              Publish all
            </button>
          )}
          <button type="button" className="btn btn-sm btn-primary" onClick={() => setEditing("new")}>
            Add target
          </button>
        </div>
      </div>

      {error && (
        <div role="alert" className="alert alert-error mb-4">
          <span>{error}</span>
        </div>
      )}
      {!targets && loading && <span className="loading loading-spinner" aria-label="Loading targets" />}
      {targets && targets.length === 0 && (
        <p className="text-base-content/60 py-8 text-center">No delivery target yet.</p>
      )}

      <div className="grid gap-4 md:grid-cols-2">
        {targets?.map((t) => {
          const provider =
            PROVIDERS.find((p) => p.id === t.provider)?.label ?? (t.kind === "AZURE_BLOB" ? "Azure Blob" : "S3");
          const manifest = publicFileUrl(t, `${t.pathPrefix ? `${t.pathPrefix}/` : ""}manifest.json`);
          return (
            <article key={t.id} className={`card bg-base-200 ${t.enabled ? "" : "opacity-60"}`}>
              <div className="card-body gap-2">
                <div className="flex items-start justify-between gap-2">
                  <div>
                    <h3 className="card-title text-lg">{t.name}</h3>
                    <p className="text-base-content/70 text-sm">
                      {provider} · <span className="font-mono">{t.bucket}</span>
                      {t.pathPrefix && <span className="font-mono">/{t.pathPrefix}</span>}
                    </p>
                  </div>
                  <div className="flex flex-wrap justify-end gap-1">
                    {!t.enabled && <span className="badge badge-sm">disabled</span>}
                    {t.autoPublish && <span className="badge badge-sm badge-info">auto-publish</span>}
                  </div>
                </div>
                <div className="flex flex-wrap gap-1">
                  {t.formats.map((f) => (
                    <span key={f} className="badge badge-sm badge-outline">
                      {FORMAT_LABELS[f]}
                    </span>
                  ))}
                </div>
                <p className="text-sm">
                  {t.lastError ? (
                    <span className="text-error">Last publish failed: {t.lastError}</span>
                  ) : t.lastPublishedAt ? (
                    <span className="text-base-content/70">
                      Published {formatDate(t.lastPublishedAt)} · {t.lastFileCount} files
                    </span>
                  ) : (
                    <span className="text-base-content/60">Never published</span>
                  )}
                </p>
                {manifest && (
                  <a className="link link-hover text-xs break-all" href={manifest} target="_blank" rel="noreferrer">
                    {manifest}
                  </a>
                )}
                <div className="card-actions justify-end">
                  <button type="button" className="btn btn-sm btn-ghost text-error" onClick={() => setRemoving(t)}>
                    Delete
                  </button>
                  <button type="button" className="btn btn-sm btn-ghost" onClick={() => setEditing(t)}>
                    Edit
                  </button>
                  <button
                    type="button"
                    className="btn btn-sm btn-primary"
                    disabled={publishing !== null}
                    onClick={() => void run(t.id)}
                  >
                    <ButtonSpinner busy={publishing === t.id} />
                    Publish
                  </button>
                </div>
              </div>
            </article>
          );
        })}
      </div>

      {editing && (
        <TargetModal
          target={editing === "new" ? null : editing}
          overview={overview}
          onClose={() => setEditing(null)}
          onSaved={(t) => {
            setData((all) => (all.some((x) => x.id === t.id) ? all.map((x) => (x.id === t.id ? t : x)) : [...all, t]));
            toast.success(`Target “${t.name}” saved.`);
          }}
        />
      )}
      {removing && (
        <ConfirmModal
          title={`Delete “${removing.name}”?`}
          confirmLabel="Delete target"
          onClose={() => setRemoving(null)}
          onConfirm={async () => {
            await deleteTarget(removing.id);
            setData((all) => all.filter((x) => x.id !== removing.id));
          }}
        >
          Files already published stay in the bucket; Tess just stops publishing there.
        </ConfirmModal>
      )}
    </section>
  );
}
