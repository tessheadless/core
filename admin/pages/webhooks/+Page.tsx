// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState } from "react";
import { ConfirmModal } from "../../components/ConfirmModal";
import { Modal } from "../../components/Modal";
import { Restricted } from "../../components/Restricted";
import { useToast } from "../../components/Toasts";
import { formatDate } from "../../lib/format";
import { describeError } from "../../lib/graphql";
import { useQuery } from "../../lib/useQuery";
import { useViewer } from "../../lib/viewer";
import {
  deleteWebhook,
  fetchWebhooksPage,
  rotateSecret,
  testWebhook,
  updateWebhook,
  type Webhook,
} from "../../lib/webhooks";
import { DeliveriesDrawer } from "./DeliveriesDrawer";
import { WebhookModal } from "./WebhookModal";

export default function Page() {
  const { isAdmin } = useViewer();
  return (
    <Restricted allowed={isAdmin} what="manage webhooks">
      <Webhooks />
    </Restricted>
  );
}

function Webhooks() {
  const toast = useToast();
  const { data, error, loading, reload, setData } = useQuery(fetchWebhooksPage);
  const [editing, setEditing] = useState<Webhook | "new" | null>(null);
  const [removing, setRemoving] = useState<Webhook | null>(null);
  const [logFor, setLogFor] = useState<Webhook | null>(null);
  const [secret, setSecret] = useState<{ name: string; value: string } | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  function upsert(w: Webhook) {
    setData((d) => ({
      ...d,
      webhooks: d.webhooks.some((x) => x.id === w.id)
        ? d.webhooks.map((x) => (x.id === w.id ? w : x))
        : [...d.webhooks, w],
    }));
  }

  async function act(w: Webhook, what: "test" | "toggle" | "rotate") {
    setBusy(`${what}:${w.id}`);
    try {
      if (what === "test") {
        const d = await testWebhook(w.id);
        if (d.status === "SUCCESS")
          toast.success(`${w.name}: test delivered (HTTP ${d.responseStatus}, ${d.durationMs} ms).`);
        else toast.error(`${w.name}: ${d.error ?? "test failed"}`);
      } else if (what === "toggle") {
        upsert(await updateWebhook(w.id, { enabled: !w.enabled }));
      } else {
        setSecret({ name: w.name, value: await rotateSecret(w.id) });
      }
    } catch (e) {
      toast.error(describeError(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <>
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-3xl font-bold">Webhooks</h1>
        <div className="flex gap-2">
          <button type="button" className="btn btn-ghost btn-sm" onClick={reload} disabled={loading}>
            Refresh
          </button>
          <button type="button" className="btn btn-primary btn-sm" onClick={() => setEditing("new")} disabled={!data}>
            Add webhook
          </button>
        </div>
      </div>
      <p className="text-base-content/70 mb-6 max-w-3xl text-sm">
        Tess POSTs a signed JSON event to every subscribed webhook when content, models or translations change — for
        example to start an n8n workflow. Verify the <code>X-Tess-Signature</code> header (
        <code>t=…,v1=HMAC-SHA256(secret, "t.body")</code>). Failed deliveries are retried up to 5 times.
      </p>

      {error && (
        <div role="alert" className="alert alert-error mb-4">
          <span>Could not load webhooks: {error}</span>
        </div>
      )}
      {data && data.webhooks.length === 0 && <p className="text-base-content/60 py-8 text-center">No webhook yet.</p>}
      {data && data.webhooks.length > 0 && (
        <div className="overflow-x-auto">
          <table className="table">
            <thead>
              <tr>
                <th>Webhook</th>
                <th>Events</th>
                <th>Enabled</th>
                <th className="sr-only">Actions</th>
              </tr>
            </thead>
            <tbody>
              {data.webhooks.map((w) => (
                <tr key={w.id}>
                  <td className="max-w-md">
                    <div className="flex items-center gap-2 font-semibold">
                      {w.name}
                      {w.managedBy && <span className="badge badge-sm badge-info">{w.managedBy}</span>}
                    </div>
                    <div className="text-base-content/70 font-mono text-xs break-all">{w.url}</div>
                    <div className="text-base-content/50 text-xs">
                      by {w.createdBy} · {formatDate(w.createdAt)}
                      {w.contentTypes.length > 0 && ` · ${w.contentTypes.join(", ")}`}
                    </div>
                  </td>
                  <td>
                    <div className="flex max-w-xs flex-wrap gap-1">
                      {w.events.map((e) => (
                        <span key={e} className="badge badge-sm badge-outline font-mono">
                          {e}
                        </span>
                      ))}
                    </div>
                  </td>
                  <td>
                    <input
                      type="checkbox"
                      className="toggle toggle-sm toggle-success"
                      checked={w.enabled}
                      disabled={busy === `toggle:${w.id}`}
                      onChange={() => void act(w, "toggle")}
                      aria-label={`Enable ${w.name}`}
                    />
                  </td>
                  {/* Every action is a labelled button; the linter flags the cell itself. */}
                  {/* oxlint-disable-next-line jsx-a11y/control-has-associated-label */}
                  <td>
                    <div className="flex justify-end gap-1">
                      <button
                        type="button"
                        className="btn btn-xs"
                        disabled={busy !== null}
                        onClick={() => void act(w, "test")}
                      >
                        {busy === `test:${w.id}` && <span className="loading loading-spinner loading-xs" />}
                        Test
                      </button>
                      <button type="button" className="btn btn-xs btn-ghost" onClick={() => setLogFor(w)}>
                        Deliveries
                      </button>
                      <button type="button" className="btn btn-xs btn-ghost" onClick={() => setEditing(w)}>
                        Edit
                      </button>
                      <details className="dropdown dropdown-end">
                        <summary className="btn btn-xs btn-ghost" aria-label="More actions">
                          ⋯
                        </summary>
                        <ul className="dropdown-content menu bg-base-100 rounded-box z-10 w-48 p-2 shadow">
                          <li>
                            <button type="button" onClick={() => void act(w, "rotate")}>
                              Rotate secret
                            </button>
                          </li>
                          <li>
                            <button type="button" className="text-error" onClick={() => setRemoving(w)}>
                              Delete
                            </button>
                          </li>
                        </ul>
                      </details>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {editing && data && (
        <WebhookModal
          webhook={editing === "new" ? null : editing}
          events={data.webhookEvents}
          contentTypes={data.contentTypes.map((c) => c.apiId)}
          onClose={() => setEditing(null)}
          onSaved={(w, newSecret) => {
            upsert(w);
            if (newSecret) setSecret({ name: w.name, value: newSecret });
            else toast.success(`Webhook “${w.name}” saved.`);
          }}
        />
      )}
      {logFor && <DeliveriesDrawer webhook={logFor} onClose={() => setLogFor(null)} />}
      {removing && (
        <ConfirmModal
          title={`Delete “${removing.name}”?`}
          confirmLabel="Delete webhook"
          onClose={() => setRemoving(null)}
          onConfirm={async () => {
            await deleteWebhook(removing.id);
            setData((d) => ({ ...d, webhooks: d.webhooks.filter((x) => x.id !== removing.id) }));
          }}
        >
          {removing.managedBy
            ? `It was registered by ${removing.managedBy}: deactivate the workflow there, or it may register again.`
            : "Tess stops sending events to it."}
        </ConfirmModal>
      )}
      {secret && (
        <Modal title="Signing secret" onClose={() => setSecret(null)}>
          {(close) => (
            <>
              <p className="py-2 text-sm">
                Copy the secret of <strong>{secret.name}</strong> now: it will not be shown again. Use it to verify{" "}
                <code>X-Tess-Signature</code>.
              </p>
              <input
                className="input w-full font-mono"
                readOnly
                value={secret.value}
                onFocus={(e) => e.target.select()}
                aria-label="Signing secret"
              />
              <div className="modal-action">
                <button type="button" className="btn" onClick={() => void navigator.clipboard?.writeText(secret.value)}>
                  Copy
                </button>
                <button type="button" className="btn btn-primary" onClick={close}>
                  Done
                </button>
              </div>
            </>
          )}
        </Modal>
      )}
    </>
  );
}
