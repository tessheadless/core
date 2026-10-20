// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState, type FormEvent } from "react";
import { ButtonSpinner, TextField } from "../../components/Field";
import { Modal } from "../../components/Modal";
import { describeError } from "../../lib/graphql";
import { createWebhook, groupEvents, SUBJECT_LABELS, updateWebhook, type Webhook } from "../../lib/webhooks";

interface Props {
  webhook: Webhook | null;
  events: string[];
  contentTypes: string[];
  onClose: () => void;
  /** `secret` is set for new webhooks (shown once). */
  onSaved: (w: Webhook, secret?: string) => void;
}

export function WebhookModal({ webhook, events, contentTypes, onClose, onSaved }: Props) {
  const [name, setName] = useState(webhook?.name ?? "");
  const [url, setUrl] = useState(webhook?.url ?? "");
  const [selected, setSelected] = useState<string[]>(webhook?.events ?? ["entry.publish"]);
  const [types, setTypes] = useState<string[]>(webhook?.contentTypes ?? []);
  const [headers, setHeaders] = useState<{ name: string; value: string }[]>(
    webhook?.headers.map((h) => ({ name: h, value: "" })) ?? [],
  );
  const [headersTouched, setHeadersTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const groups = groupEvents(events.filter((e) => e !== "webhook.ping"));

  const wildcard = (subject: string) => `${subject}.*`;
  const has = (e: string) => selected.includes(e) || selected.includes("*");
  const toggle = (e: string, on: boolean) =>
    setSelected((s) => (on ? [...s.filter((x) => x !== e), e] : s.filter((x) => x !== e)));

  async function submit(ev: FormEvent, close: () => void) {
    ev.preventDefault();
    setBusy(true);
    setError(null);
    const base = {
      name: name.trim(),
      url: url.trim(),
      enabled: webhook?.enabled ?? true,
      events: selected,
      contentTypes: types,
    };
    const cleanHeaders = headers.filter((h) => h.name.trim()).map((h) => ({ name: h.name.trim(), value: h.value }));
    if (headersTouched && cleanHeaders.some((h) => !h.value)) {
      setError(
        "Give a value to every header (stored values are write-only, so they must be re-entered when headers change).",
      );
      setBusy(false);
      return;
    }
    try {
      if (webhook) {
        const w = await updateWebhook(webhook.id, headersTouched ? { ...base, headers: cleanHeaders } : base);
        close();
        onSaved(w);
      } else {
        const { webhook: w, secret } = await createWebhook({ ...base, headers: cleanHeaders });
        close();
        onSaved(w, secret);
      }
    } catch (err) {
      setError(describeError(err));
      setBusy(false);
    }
  }

  return (
    <Modal title={webhook ? `Edit “${webhook.name}”` : "Add a webhook"} onClose={onClose} wide>
      {(close) => (
        <form onSubmit={(e) => void submit(e, close)} className="mt-2">
          <TextField label="Name" value={name} onChange={setName} required />
          <TextField
            label="URL"
            value={url}
            onChange={setUrl}
            mono
            required
            type="url"
            placeholder="http://n8n:5678/n8n/webhook/…"
            hint="Receives POST requests with a JSON body. Redirects are not followed."
          />

          <fieldset className="fieldset mt-2">
            <legend className="fieldset-legend">Events</legend>
            <label className="label cursor-pointer justify-start gap-2">
              <input
                type="checkbox"
                className="checkbox checkbox-sm"
                checked={selected.includes("*")}
                onChange={(e) => toggle("*", e.target.checked)}
              />
              Everything (<code>*</code>)
            </label>
            <div className="grid gap-3 sm:grid-cols-2">
              {groups.map(([subject, list]) => (
                <div key={subject} className="bg-base-200 rounded p-2">
                  <label className="label cursor-pointer justify-start gap-2 font-semibold">
                    <input
                      type="checkbox"
                      className="checkbox checkbox-xs"
                      checked={has(wildcard(subject))}
                      disabled={selected.includes("*")}
                      onChange={(e) => toggle(wildcard(subject), e.target.checked)}
                    />
                    {SUBJECT_LABELS[subject] ?? subject}{" "}
                    <span className="font-mono text-xs font-normal">{wildcard(subject)}</span>
                  </label>
                  <div className="flex flex-wrap gap-x-3 pl-6">
                    {list.map((e) => (
                      <label key={e} className="label cursor-pointer gap-1">
                        <input
                          type="checkbox"
                          className="checkbox checkbox-xs"
                          checked={has(e) || has(wildcard(subject))}
                          disabled={has(wildcard(subject))}
                          onChange={(ev) => toggle(e, ev.target.checked)}
                        />
                        <span className="font-mono text-xs">{e.slice(subject.length + 1)}</span>
                      </label>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </fieldset>

          <fieldset className="fieldset">
            <legend className="fieldset-legend">Content types (none = all)</legend>
            <div className="flex flex-wrap gap-3">
              {contentTypes.map((ct) => (
                <label key={ct} className="label cursor-pointer gap-1">
                  <input
                    type="checkbox"
                    className="checkbox checkbox-xs"
                    checked={types.includes(ct)}
                    onChange={(e) => setTypes((t) => (e.target.checked ? [...t, ct] : t.filter((x) => x !== ct)))}
                  />
                  <span className="font-mono text-sm">{ct}</span>
                </label>
              ))}
            </div>
          </fieldset>

          <fieldset className="fieldset">
            <legend className="fieldset-legend">Extra headers</legend>
            <p className="text-base-content/70 text-xs">
              Values are encrypted and never shown again, e.g. an n8n header-auth credential.
            </p>
            {headers.map((h, i) => (
              <div key={i} className="flex gap-2">
                <input
                  className="input input-sm w-1/3 font-mono"
                  placeholder="X-Api-Key"
                  value={h.name}
                  aria-label="Header name"
                  onChange={(e) => {
                    setHeadersTouched(true);
                    setHeaders((hs) => hs.map((x, j) => (j === i ? { ...x, name: e.target.value } : x)));
                  }}
                />
                <input
                  className="input input-sm flex-1 font-mono"
                  type="password"
                  autoComplete="off"
                  placeholder={webhook && !headersTouched ? "•••••• (unchanged)" : "value"}
                  value={h.value}
                  aria-label="Header value"
                  onChange={(e) => {
                    setHeadersTouched(true);
                    setHeaders((hs) => hs.map((x, j) => (j === i ? { ...x, value: e.target.value } : x)));
                  }}
                />
                <button
                  type="button"
                  className="btn btn-sm btn-ghost"
                  aria-label="Remove header"
                  onClick={() => {
                    setHeadersTouched(true);
                    setHeaders((hs) => hs.filter((_, j) => j !== i));
                  }}
                >
                  ✕
                </button>
              </div>
            ))}
            <button
              type="button"
              className="btn btn-xs btn-ghost w-fit"
              onClick={() => {
                setHeadersTouched(true);
                setHeaders((hs) => [...hs, { name: "", value: "" }]);
              }}
            >
              + Add header
            </button>
          </fieldset>

          {error && <p className="text-error mt-2 text-sm">{error}</p>}
          <div className="modal-action">
            <button type="button" className="btn btn-ghost" onClick={close}>
              Cancel
            </button>
            <button type="submit" className="btn btn-primary" disabled={busy || selected.length === 0}>
              <ButtonSpinner busy={busy} />
              Save
            </button>
          </div>
        </form>
      )}
    </Modal>
  );
}
