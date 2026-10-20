// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { formatBytes, formatDate } from "../../lib/format";
import type { Plugin } from "../../lib/plugins";
import { StatusBadge } from "./StatusBadge";

interface PluginTableProps {
  plugins: Plugin[];
  pendingToggles: ReadonlySet<string>;
  onToggle: (plugin: Plugin, enabled: boolean) => void;
  onSettings: (name: string) => void;
  onUninstall: (name: string) => void;
}

export function PluginTable({ plugins, pendingToggles, onToggle, onSettings, onUninstall }: PluginTableProps) {
  return (
    <div className="border-base-300 overflow-x-auto rounded-box border">
      <table className="table">
        <caption className="sr-only">Installed plugins</caption>
        <thead>
          <tr>
            <th scope="col">Plugin</th>
            <th scope="col">Enabled</th>
            <th scope="col">Status</th>
            <th scope="col">Hooks</th>
            <th scope="col">Content types</th>
            <th scope="col">Size</th>
            <th scope="col">Updated</th>
            <th scope="col">
              <span className="sr-only">Actions</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {plugins.map((p) => (
            <tr key={p.name} className="align-top">
              <td className="min-w-48">
                <div className="font-mono font-semibold">{p.name}</div>
                {p.version && <div className="text-base-content/60 text-xs">v{p.version}</div>}
                {p.description && <div className="text-base-content/70 mt-1 max-w-xs text-sm">{p.description}</div>}
              </td>
              <td>
                <input
                  type="checkbox"
                  className="toggle toggle-success toggle-sm"
                  checked={p.enabled}
                  disabled={pendingToggles.has(p.name)}
                  onChange={(e) => onToggle(p, e.target.checked)}
                  aria-label={`Enable ${p.name}`}
                />
              </td>
              <td className="min-w-32">
                <StatusBadge plugin={p} />
                {p.lastError && (
                  <p className="text-error mt-1 line-clamp-3 max-w-xs text-xs break-words" title={p.lastError}>
                    {p.lastError}
                  </p>
                )}
              </td>
              <td>
                {p.exports.length > 0 ? (
                  <div className="flex flex-wrap gap-1">
                    {p.exports.map((hook) => (
                      <span key={hook} className="badge badge-outline badge-sm font-mono">
                        {hook}
                      </span>
                    ))}
                  </div>
                ) : (
                  <span className="text-base-content/60">none</span>
                )}
              </td>
              <td>
                {p.contentTypes.length > 0 ? (
                  <span className="font-mono text-sm">{p.contentTypes.join(", ")}</span>
                ) : (
                  <span className="badge badge-ghost badge-sm">All</span>
                )}
              </td>
              <td className="whitespace-nowrap">{formatBytes(p.size)}</td>
              <td className="whitespace-nowrap">{formatDate(p.updatedAt)}</td>
              <td>
                <div className="flex justify-end gap-1">
                  <button
                    type="button"
                    className="btn btn-sm"
                    onClick={() => onSettings(p.name)}
                    aria-label={`Settings of ${p.name}`}
                  >
                    Settings
                  </button>
                  <button
                    type="button"
                    className="btn btn-sm btn-ghost text-error"
                    onClick={() => onUninstall(p.name)}
                    aria-label={`Uninstall ${p.name}`}
                  >
                    Uninstall
                  </button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
