// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useEffect, useState } from "react";
import { Modal } from "../../components/Modal";
import { formatDate } from "../../lib/format";
import { describeError } from "../../lib/graphql";
import { fetchDeliveries, type Webhook, type WebhookDelivery } from "../../lib/webhooks";

const STATUS_CLASS = { SUCCESS: "badge-success", FAILED: "badge-error", RETRYING: "badge-warning" } as const;

export function DeliveriesDrawer({ webhook, onClose }: { webhook: Webhook; onClose: () => void }) {
  const [items, setItems] = useState<WebhookDelivery[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    let cancelled = false;
    fetchDeliveries(webhook.id).then(
      (d) => !cancelled && setItems(d),
      (e: unknown) => !cancelled && setError(describeError(e)),
    );
    return () => {
      cancelled = true;
    };
  }, [webhook.id, version]);

  return (
    <Modal title={`Deliveries · ${webhook.name}`} onClose={onClose} variant="drawer">
      {() => (
        <div className="mt-4">
          <div className="mb-2 flex justify-between">
            <p className="text-base-content/70 text-sm">The last 25 deliveries, most recent first.</p>
            <button type="button" className="btn btn-xs" onClick={() => setVersion((v) => v + 1)}>
              Refresh
            </button>
          </div>
          {error && <p className="text-error text-sm">{error}</p>}
          {!items && !error && <span className="loading loading-spinner" aria-label="Loading deliveries" />}
          {items?.length === 0 && <p className="text-base-content/60 py-6 text-center">Nothing delivered yet.</p>}
          <ul className="flex flex-col gap-2">
            {items?.map((d) => (
              <li key={d.id} className="bg-base-200 rounded p-3">
                <div className="flex flex-wrap items-center gap-2">
                  <span className={`badge badge-sm ${STATUS_CLASS[d.status]}`}>{d.status.toLowerCase()}</span>
                  <span className="font-mono text-sm">{d.event}</span>
                  <span className="text-base-content/60 ml-auto text-xs">{formatDate(d.createdAt)}</span>
                </div>
                <div className="text-base-content/70 mt-1 text-xs">
                  {d.responseStatus !== null && `HTTP ${d.responseStatus} · `}
                  {d.durationMs} ms · {d.attempts} attempt{d.attempts === 1 ? "" : "s"} ·{" "}
                  <span className="font-mono">{d.id}</span>
                </div>
                {d.error && <pre className="text-error mt-1 text-xs whitespace-pre-wrap">{d.error}</pre>}
              </li>
            ))}
          </ul>
        </div>
      )}
    </Modal>
  );
}
