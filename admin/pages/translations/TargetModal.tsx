// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState, type FormEvent } from "react";
import { ButtonSpinner, TextField } from "../../components/Field";
import { Modal } from "../../components/Modal";
import { describeError } from "../../lib/graphql";
import {
  FORMAT_LABELS,
  PROVIDERS,
  saveTarget,
  type AzureAuth,
  type DeliveryTarget,
  type OverviewData,
  type TargetInput,
  type TranslationFormat,
  type TranslationState,
} from "../../lib/i18n";

interface Props {
  target: DeliveryTarget | null;
  overview: OverviewData;
  onClose: () => void;
  onSaved: (t: DeliveryTarget) => void;
}

export function TargetModal({ target, overview, onClose, onSaved }: Props) {
  const initialProvider = PROVIDERS.find((p) => p.id === target?.provider) ?? PROVIDERS[0];
  const [provider, setProvider] = useState(initialProvider.id);
  const preset = PROVIDERS.find((p) => p.id === provider) ?? PROVIDERS[0];
  const kind = target?.kind ?? preset.kind;
  const [form, setForm] = useState({
    name: target?.name ?? "",
    enabled: target?.enabled ?? true,
    bucket: target?.bucket ?? "",
    endpoint: target?.endpoint ?? initialProvider.endpoint ?? "",
    region: target?.region ?? initialProvider.region ?? "",
    forcePathStyle: target?.forcePathStyle ?? initialProvider.forcePathStyle ?? false,
    accessKeyId: target?.accessKeyId ?? "",
    azureAuth: (target?.azureAuth ?? "SHARED_KEY") as AzureAuth,
    pathPrefix: target?.pathPrefix ?? "",
    publicUrl: target?.publicUrl ?? "",
    formats: target?.formats ?? (["JSON_NESTED"] as TranslationFormat[]),
    languages: target?.languages ?? [],
    reviewedOnly: target ? target.states.length === 1 && target.states[0] === "REVIEWED" : false,
    fallbackToBase: target?.fallbackToBase ?? false,
    autoPublish: target?.autoPublish ?? false,
  });
  const [secret, setSecret] = useState("");
  const [clearSecret, setClearSecret] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const set = <K extends keyof typeof form>(k: K, v: (typeof form)[K]) => setForm((f) => ({ ...f, [k]: v }));

  function choose(id: string) {
    const p = PROVIDERS.find((x) => x.id === id);
    if (!p) return;
    setProvider(id);
    if (!target) {
      setForm((f) => ({
        ...f,
        endpoint: p.endpoint ?? "",
        region: p.region ?? "",
        forcePathStyle: p.forcePathStyle ?? false,
      }));
    }
  }

  async function submit(e: FormEvent, close: () => void) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    const states: TranslationState[] = form.reviewedOnly ? ["REVIEWED"] : ["TRANSLATED", "REVIEWED"];
    const input: TargetInput = {
      name: form.name.trim(),
      enabled: form.enabled,
      kind,
      provider,
      bucket: form.bucket.trim(),
      endpoint: form.endpoint.trim() || null,
      region: kind === "S3" ? form.region.trim() || null : null,
      forcePathStyle: form.forcePathStyle,
      accessKeyId: form.accessKeyId.trim() || null,
      azureAuth: form.azureAuth,
      pathPrefix: form.pathPrefix.trim(),
      publicUrl: form.publicUrl.trim() || null,
      formats: form.formats,
      languages: form.languages,
      states,
      fallbackToBase: form.fallbackToBase,
      autoPublish: form.autoPublish,
    };
    if (secret) input.secret = secret;
    else if (clearSecret) input.secret = null;
    try {
      const saved = await saveTarget(target?.id ?? null, input);
      close();
      onSaved(saved);
    } catch (err) {
      setError(describeError(err));
      setBusy(false);
    }
  }

  const secretLabel =
    kind === "AZURE_BLOB" ? (form.azureAuth === "SAS" ? "SAS token" : "Account key") : "Secret access key";

  return (
    <Modal title={target ? `Edit “${target.name}”` : "Add a delivery target"} onClose={onClose} wide>
      {(close) => (
        <form onSubmit={(e) => void submit(e, close)} className="mt-2">
          {!target && (
            <fieldset className="mb-2 flex flex-wrap gap-2">
              <legend className="sr-only">Storage provider</legend>
              {PROVIDERS.map((p) => (
                <input
                  key={p.id}
                  type="radio"
                  name="provider"
                  className="btn btn-sm"
                  aria-label={p.label}
                  checked={provider === p.id}
                  onChange={() => choose(p.id)}
                />
              ))}
            </fieldset>
          )}
          <p className="text-base-content/70 mb-2 text-xs">{preset.hint}</p>
          <div className="grid gap-x-4 sm:grid-cols-2">
            <TextField label="Name" value={form.name} onChange={(v) => set("name", v)} required />
            <TextField
              label={kind === "AZURE_BLOB" ? "Container" : "Bucket"}
              value={form.bucket}
              onChange={(v) => set("bucket", v)}
              mono
              required
            />
            <TextField
              label="Endpoint"
              value={form.endpoint}
              onChange={(v) => set("endpoint", v)}
              mono
              hint={
                kind === "AZURE_BLOB"
                  ? "Empty for Azure; Azurite: http://127.0.0.1:10000/devstoreaccount1"
                  : "Empty for AWS S3."
              }
            />
            {kind === "S3" && <TextField label="Region" value={form.region} onChange={(v) => set("region", v)} mono />}
            <TextField
              label={kind === "AZURE_BLOB" ? "Storage account" : "Access key id"}
              value={form.accessKeyId}
              onChange={(v) => set("accessKeyId", v)}
              mono
              autoComplete="off"
            />
            <TextField
              label={secretLabel}
              value={secret}
              onChange={setSecret}
              type="password"
              autoComplete="new-password"
              placeholder={target?.hasSecret ? "•••••• (stored — leave empty to keep)" : ""}
              hint="Encrypted at rest and never shown again."
            />
            {kind === "AZURE_BLOB" && (
              <fieldset className="fieldset">
                <legend className="fieldset-legend">Authentication</legend>
                <select
                  className="select w-full"
                  value={form.azureAuth}
                  onChange={(e) => set("azureAuth", e.target.value as AzureAuth)}
                >
                  <option value="SHARED_KEY">Account key (Shared Key)</option>
                  <option value="SAS">SAS token</option>
                </select>
              </fieldset>
            )}
            <TextField
              label="Path prefix"
              value={form.pathPrefix}
              onChange={(v) => set("pathPrefix", v)}
              mono
              placeholder="i18n/web"
            />
            <TextField
              label="Public URL"
              value={form.publicUrl}
              onChange={(v) => set("publicUrl", v)}
              mono
              hint="Where apps read the files (CDN, public bucket)."
            />
          </div>
          {target?.hasSecret && !secret && (
            <label className="label cursor-pointer justify-start gap-2">
              <input
                type="checkbox"
                className="checkbox checkbox-xs"
                checked={clearSecret}
                onChange={(e) => setClearSecret(e.target.checked)}
              />
              Remove the stored secret
            </label>
          )}
          {kind === "S3" && (
            <label className="label cursor-pointer justify-start gap-2">
              <input
                type="checkbox"
                className="checkbox checkbox-sm"
                checked={form.forcePathStyle}
                onChange={(e) => set("forcePathStyle", e.target.checked)}
              />
              Path-style URLs (Garage, MinIO)
            </label>
          )}

          <fieldset className="fieldset mt-2">
            <legend className="fieldset-legend">Formats</legend>
            <div className="grid grid-cols-2 gap-1 sm:grid-cols-3">
              {overview.translationFormats.map((f) => (
                <label key={f.format} className="label cursor-pointer justify-start gap-2">
                  <input
                    type="checkbox"
                    className="checkbox checkbox-xs"
                    checked={form.formats.includes(f.format)}
                    onChange={(e) =>
                      set(
                        "formats",
                        e.target.checked ? [...form.formats, f.format] : form.formats.filter((x) => x !== f.format),
                      )
                    }
                  />
                  <span className="text-sm">{FORMAT_LABELS[f.format]}</span>
                </label>
              ))}
            </div>
          </fieldset>
          <fieldset className="fieldset">
            <legend className="fieldset-legend">Languages (none = all)</legend>
            <div className="flex flex-wrap gap-3">
              {overview.translationLanguages.map((l) => (
                <label key={l.tag} className="label cursor-pointer gap-1">
                  <input
                    type="checkbox"
                    className="checkbox checkbox-xs"
                    checked={form.languages.includes(l.tag)}
                    onChange={(e) =>
                      set(
                        "languages",
                        e.target.checked ? [...form.languages, l.tag] : form.languages.filter((x) => x !== l.tag),
                      )
                    }
                  />
                  <span className="font-mono text-sm">{l.tag}</span>
                </label>
              ))}
            </div>
          </fieldset>
          <div className="mt-2 flex flex-col gap-1">
            <label className="label cursor-pointer justify-start gap-2">
              <input
                type="checkbox"
                className="checkbox checkbox-sm"
                checked={form.reviewedOnly}
                onChange={(e) => set("reviewedOnly", e.target.checked)}
              />
              Publish reviewed translations only
            </label>
            <label className="label cursor-pointer justify-start gap-2">
              <input
                type="checkbox"
                className="checkbox checkbox-sm"
                checked={form.fallbackToBase}
                onChange={(e) => set("fallbackToBase", e.target.checked)}
              />
              Fill missing texts with the base language
            </label>
            <label className="label cursor-pointer justify-start gap-2">
              <input
                type="checkbox"
                className="checkbox checkbox-sm"
                checked={form.autoPublish}
                onChange={(e) => set("autoPublish", e.target.checked)}
              />
              Publish automatically after every change
            </label>
            <label className="label cursor-pointer justify-start gap-2">
              <input
                type="checkbox"
                className="toggle toggle-sm"
                checked={form.enabled}
                onChange={(e) => set("enabled", e.target.checked)}
              />
              Enabled
            </label>
          </div>
          {error && <p className="text-error mt-2 text-sm">{error}</p>}
          <div className="modal-action">
            <button type="button" className="btn btn-ghost" onClick={close}>
              Cancel
            </button>
            <button type="submit" className="btn btn-primary" disabled={busy || form.formats.length === 0}>
              <ButtonSpinner busy={busy} />
              Save
            </button>
          </div>
        </form>
      )}
    </Modal>
  );
}
