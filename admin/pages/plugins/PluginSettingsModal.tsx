// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useId, useState, type FormEvent, type KeyboardEvent } from "react";
import { Modal } from "../../components/Modal";
import { useToast } from "../../components/Toasts";
import { formatBytes, formatDate, pagesToMiB } from "../../lib/format";
import { describeError } from "../../lib/graphql";
import {
  updatePlugin,
  type JsonValue,
  type Plugin,
  type PluginLimits,
  type PluginSettingsInput,
} from "../../lib/plugins";
import { StatusBadge } from "./StatusBadge";

interface PluginSettingsModalProps {
  plugin: Plugin;
  limits: PluginLimits;
  contentTypeIds: string[];
  onClose: () => void;
  onSaved: (plugin: Plugin) => void;
}

// Host name with an optional leading "*." wildcard and optional port, no scheme or path.
const HOST_PATTERN = /^(\*\.)?([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)*[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(:\d{1,5})?$/;

function parseBoundedInt(value: string, max: number): number | null {
  if (!/^\d+$/.test(value)) return null;
  const n = Number(value);
  return n >= 1 && n <= max ? n : null;
}

function parseConfig(text: string): { value: JsonValue } | { error: string } {
  try {
    return { value: JSON.parse(text) as JsonValue };
  } catch (e) {
    return { error: e instanceof Error ? e.message : "Invalid JSON." };
  }
}

export function PluginSettingsModal({ plugin, limits, contentTypeIds, onClose, onSaved }: PluginSettingsModalProps) {
  const toast = useToast();
  const id = useId();
  const [enabled, setEnabled] = useState(plugin.enabled);
  const [contentTypes, setContentTypes] = useState<string[]>(plugin.contentTypes);
  const [hosts, setHosts] = useState<string[]>(plugin.allowedHosts);
  const [hostDraft, setHostDraft] = useState("");
  const [hostError, setHostError] = useState<string | null>(null);
  const [timeoutMs, setTimeoutMs] = useState(String(plugin.timeoutMs));
  const [memoryPages, setMemoryPages] = useState(String(plugin.memoryMaxPages));
  const [failOpen, setFailOpen] = useState(plugin.failOpen);
  const [configText, setConfigText] = useState(JSON.stringify(plugin.config, null, 2));
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const timeoutValue = parseBoundedInt(timeoutMs, limits.maxTimeoutMs);
  const memoryValue = parseBoundedInt(memoryPages, limits.maxMemoryPages);
  const config = parseConfig(configText);
  const configError = "error" in config ? config.error : null;
  const valid = timeoutValue !== null && memoryValue !== null && configError === null;

  // Keep scoped content types that no longer exist visible, so they are not dropped silently.
  const typeOptions = [...new Set([...contentTypeIds, ...plugin.contentTypes])].sort();

  function toggleContentType(apiId: string, checked: boolean) {
    setContentTypes((list) => (checked ? [...list, apiId] : list.filter((t) => t !== apiId)));
  }

  function addHost() {
    const host = hostDraft.trim().toLowerCase();
    if (!host) return;
    if (!HOST_PATTERN.test(host)) {
      setHostError("Enter a host name only, e.g. api.example.com or *.example.com (no scheme or path).");
      return;
    }
    if (!hosts.includes(host)) setHosts([...hosts, host]);
    setHostDraft("");
    setHostError(null);
  }

  function onHostKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter" || event.key === ",") {
      event.preventDefault();
      addHost();
    }
  }

  async function copySha() {
    try {
      await navigator.clipboard.writeText(plugin.sha256);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error("Could not copy to the clipboard.");
    }
  }

  async function submit(event: FormEvent, close: () => void) {
    event.preventDefault();
    if (!valid || "error" in config) return;
    const input: PluginSettingsInput = {
      enabled,
      contentTypes,
      timeoutMs: timeoutValue,
      memoryMaxPages: memoryValue,
      failOpen,
      config: config.value,
    };
    // When outgoing HTTP is disabled server-side, leave allowedHosts untouched.
    if (limits.allowHttp) input.allowedHosts = hosts;
    setSaving(true);
    setSaveError(null);
    try {
      const saved = await updatePlugin(plugin.name, input);
      onSaved(saved);
      toast.success(`Settings of "${plugin.name}" saved.`);
      close();
    } catch (e) {
      // Shown inline: toasts sit below the modal's top layer.
      setSaveError(describeError(e));
      setSaving(false);
    }
  }

  return (
    <Modal
      wide
      onClose={onClose}
      title={
        <span className="flex flex-wrap items-center gap-2">
          <span className="font-mono">{plugin.name}</span>
          {plugin.version && <span className="badge badge-ghost">v{plugin.version}</span>}
          <StatusBadge plugin={plugin} />
        </span>
      }
    >
      {(close) => (
        <form onSubmit={(e) => void submit(e, close)} noValidate>
          {plugin.description && <p className="text-base-content/70 mt-1 text-sm">{plugin.description}</p>}
          {plugin.lastError && (
            <div role="alert" className="alert alert-error alert-soft mt-3 text-sm">
              <div>
                <p className="font-semibold">Last error</p>
                <pre className="font-mono text-xs break-words whitespace-pre-wrap">{plugin.lastError}</pre>
              </div>
            </div>
          )}

          <dl className="mt-4 grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-sm">
            <dt className="text-base-content/60">SHA-256</dt>
            <dd className="flex items-center gap-2">
              <code className="font-mono" title={plugin.sha256}>
                {plugin.sha256.slice(0, 16)}…
              </code>
              <button type="button" className="btn btn-ghost btn-xs" onClick={() => void copySha()}>
                {copied ? "Copied" : "Copy"}
              </button>
            </dd>
            <dt className="text-base-content/60">Size</dt>
            <dd>{formatBytes(plugin.size)}</dd>
            <dt className="text-base-content/60">Hooks</dt>
            <dd className="font-mono">{plugin.exports.length > 0 ? plugin.exports.join(", ") : "none"}</dd>
            <dt className="text-base-content/60">Installed</dt>
            <dd>{formatDate(plugin.installedAt)}</dd>
            <dt className="text-base-content/60">Updated</dt>
            <dd>{formatDate(plugin.updatedAt)}</dd>
          </dl>

          <div className="divider" />

          <label className="label cursor-pointer gap-3">
            <input
              type="checkbox"
              className="toggle toggle-success"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
            />
            <span className="text-base-content font-medium">Enabled</span>
          </label>

          <fieldset className="fieldset mt-4">
            <legend className="fieldset-legend">Content types</legend>
            <p className="text-base-content/60 text-xs">
              The plugin runs for writes on the selected types. None selected means <strong>all</strong> content types.
            </p>
            {typeOptions.length === 0 ? (
              <p className="text-sm">No content types defined.</p>
            ) : (
              <div className="grid grid-cols-1 gap-1 sm:grid-cols-2">
                {typeOptions.map((apiId) => (
                  <label key={apiId} className="label cursor-pointer gap-2">
                    <input
                      type="checkbox"
                      className="checkbox checkbox-sm"
                      checked={contentTypes.includes(apiId)}
                      onChange={(e) => toggleContentType(apiId, e.target.checked)}
                    />
                    <span className="text-base-content font-mono">
                      {apiId}
                      {!contentTypeIds.includes(apiId) && <span className="text-warning"> (missing)</span>}
                    </span>
                  </label>
                ))}
              </div>
            )}
            {contentTypes.length > 0 && (
              <button type="button" className="btn btn-ghost btn-xs w-fit" onClick={() => setContentTypes([])}>
                Clear selection (all content types)
              </button>
            )}
          </fieldset>

          <fieldset className="fieldset mt-4">
            <legend className="fieldset-legend">Allowed outgoing hosts</legend>
            {limits.allowHttp ? (
              <>
                <div role="note" className="alert alert-warning alert-soft text-xs">
                  Each host listed here lets the plugin make outgoing HTTP requests to it, possibly with content data.
                  Only add hosts you trust.
                </div>
                {hosts.length > 0 && (
                  <ul className="flex list-none flex-wrap gap-2 p-0" aria-label="Allowed hosts">
                    {hosts.map((host) => (
                      <li key={host} className="badge badge-outline gap-1 font-mono">
                        {host}
                        <button
                          type="button"
                          className="btn btn-ghost btn-xs px-1"
                          aria-label={`Remove ${host}`}
                          onClick={() => setHosts(hosts.filter((h) => h !== host))}
                        >
                          ✕
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
                <div className="join w-full">
                  <label htmlFor={`${id}-host`} className="sr-only">
                    Host to allow
                  </label>
                  <input
                    id={`${id}-host`}
                    type="text"
                    className={`input join-item w-full font-mono ${hostError ? "input-error" : ""}`}
                    placeholder="api.example.com"
                    value={hostDraft}
                    autoComplete="off"
                    spellCheck={false}
                    onChange={(e) => {
                      setHostDraft(e.target.value);
                      setHostError(null);
                    }}
                    onKeyDown={onHostKeyDown}
                    aria-invalid={hostError ? true : undefined}
                    aria-describedby={hostError ? `${id}-host-error` : undefined}
                  />
                  <button type="button" className="btn join-item" onClick={addHost}>
                    Add
                  </button>
                </div>
                {hostError && (
                  <p id={`${id}-host-error`} className="text-error text-xs">
                    {hostError}
                  </p>
                )}
              </>
            ) : (
              <p className="text-base-content/60 text-sm">
                Outgoing HTTP is disabled on this server, so plugins cannot make network requests.
                {plugin.allowedHosts.length > 0 && ` Stored hosts (inactive): ${plugin.allowedHosts.join(", ")}.`}
              </p>
            )}
          </fieldset>

          <div className="mt-4 grid gap-4 sm:grid-cols-2">
            <fieldset className="fieldset">
              <label className="fieldset-label" htmlFor={`${id}-timeout`}>
                Timeout (ms)
              </label>
              <input
                id={`${id}-timeout`}
                type="number"
                inputMode="numeric"
                className={`input w-full ${timeoutValue === null ? "input-error" : ""}`}
                min={1}
                max={limits.maxTimeoutMs}
                step={1}
                value={timeoutMs}
                onChange={(e) => setTimeoutMs(e.target.value)}
                aria-invalid={timeoutValue === null ? true : undefined}
                aria-describedby={`${id}-timeout-help`}
              />
              <p
                id={`${id}-timeout-help`}
                className={`text-xs ${timeoutValue === null ? "text-error" : "text-base-content/60"}`}
              >
                Between 1 and {limits.maxTimeoutMs} ms.
              </p>
            </fieldset>
            <fieldset className="fieldset">
              <label className="fieldset-label" htmlFor={`${id}-memory`}>
                Memory limit (64 KiB pages)
              </label>
              <input
                id={`${id}-memory`}
                type="number"
                inputMode="numeric"
                className={`input w-full ${memoryValue === null ? "input-error" : ""}`}
                min={1}
                max={limits.maxMemoryPages}
                step={1}
                value={memoryPages}
                onChange={(e) => setMemoryPages(e.target.value)}
                aria-invalid={memoryValue === null ? true : undefined}
                aria-describedby={`${id}-memory-help`}
              />
              <p
                id={`${id}-memory-help`}
                className={`text-xs ${memoryValue === null ? "text-error" : "text-base-content/60"}`}
              >
                {memoryValue !== null ? `= ${pagesToMiB(memoryValue)}. ` : ""}
                Between 1 and {limits.maxMemoryPages} pages ({pagesToMiB(limits.maxMemoryPages)}).
              </p>
            </fieldset>
          </div>

          <fieldset className="fieldset mt-4">
            <label className="label cursor-pointer items-start gap-3">
              <input
                type="checkbox"
                className="checkbox mt-0.5"
                checked={failOpen}
                onChange={(e) => setFailOpen(e.target.checked)}
                aria-describedby={`${id}-failopen-help`}
              />
              <span className="text-base-content font-medium">Fail open</span>
            </label>
            <p id={`${id}-failopen-help`} className="text-base-content/60 text-xs">
              If the plugin crashes, allow the write instead of rejecting it.
            </p>
          </fieldset>

          <fieldset className="fieldset mt-4">
            <label className="fieldset-label" htmlFor={`${id}-config`}>
              Configuration (JSON)
            </label>
            <textarea
              id={`${id}-config`}
              className={`textarea h-40 w-full font-mono text-xs ${configError ? "textarea-error" : ""}`}
              value={configText}
              spellCheck={false}
              onChange={(e) => setConfigText(e.target.value)}
              aria-invalid={configError ? true : undefined}
              aria-describedby={`${id}-config-help`}
            />
            <p id={`${id}-config-help`} className={`text-xs ${configError ? "text-error" : "text-base-content/60"}`}>
              {configError ? `Invalid JSON: ${configError}` : "Passed to the plugin as-is. Do not store secrets here."}
            </p>
          </fieldset>

          {saveError && (
            <div role="alert" className="alert alert-error mt-4 text-sm">
              {saveError}
            </div>
          )}

          <div className="modal-action">
            <button type="button" className="btn" onClick={close}>
              Cancel
            </button>
            <button type="submit" className="btn btn-primary" disabled={!valid || saving}>
              {saving && <span className="loading loading-spinner loading-sm" aria-hidden="true" />}
              Save
            </button>
          </div>
        </form>
      )}
    </Modal>
  );
}
