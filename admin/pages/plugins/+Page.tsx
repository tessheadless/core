// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState } from "react";
import { useToast } from "../../components/Toasts";
import { describeError } from "../../lib/graphql";
import { fetchPluginsPage, updatePlugin, type Plugin } from "../../lib/plugins";
import { useQuery } from "../../lib/useQuery";
import { InstallForm } from "./InstallForm";
import { PluginSettingsModal } from "./PluginSettingsModal";
import { PluginTable } from "./PluginTable";
import { UninstallModal } from "./UninstallModal";

export default function Page() {
  const toast = useToast();
  const { data, error, loading, reload, setData } = useQuery(fetchPluginsPage);
  const [settingsFor, setSettingsFor] = useState<string | null>(null);
  const [uninstallFor, setUninstallFor] = useState<string | null>(null);
  const [pendingToggles, setPendingToggles] = useState<ReadonlySet<string>>(new Set());

  function upsert(plugin: Plugin) {
    setData((d) => {
      const exists = d.plugins.some((p) => p.name === plugin.name);
      const plugins = exists
        ? d.plugins.map((p) => (p.name === plugin.name ? plugin : p))
        : [...d.plugins, plugin].sort((a, b) => a.name.localeCompare(b.name));
      return { ...d, plugins };
    });
  }

  function remove(name: string) {
    setData((d) => ({ ...d, plugins: d.plugins.filter((p) => p.name !== name) }));
  }

  async function toggle(plugin: Plugin, enabled: boolean) {
    setPendingToggles((s) => new Set(s).add(plugin.name));
    try {
      const updated = await updatePlugin(plugin.name, { enabled });
      upsert(updated);
      if (updated.lastError) toast.error(`"${plugin.name}" reported an error: ${updated.lastError}`);
      else toast.success(`Plugin "${plugin.name}" ${enabled ? "enabled" : "disabled"}.`);
    } catch (e) {
      toast.error(`Could not update "${plugin.name}": ${describeError(e)}`);
    } finally {
      setPendingToggles((s) => {
        const next = new Set(s);
        next.delete(plugin.name);
        return next;
      });
    }
  }

  const settingsPlugin = data?.plugins.find((p) => p.name === settingsFor);

  return (
    <>
      <div className="mb-6 flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-3xl font-bold">Plugins</h1>
        <button type="button" className="btn btn-ghost btn-sm" onClick={reload} disabled={loading}>
          {loading && <span className="loading loading-spinner loading-xs" aria-hidden="true" />}
          Refresh
        </button>
      </div>

      {error && (
        <div role="alert" className="alert alert-error mb-4">
          <span>Could not load plugins: {error}</span>
          <button type="button" className="btn btn-sm" onClick={reload}>
            Retry
          </button>
        </div>
      )}

      {!data && loading && (
        <div className="flex justify-center py-12">
          <span className="loading loading-spinner loading-lg" aria-label="Loading plugins" />
        </div>
      )}

      {data && (
        <>
          <InstallForm
            limits={data.pluginLimits}
            existingNames={data.plugins.map((p) => p.name)}
            onInstalled={(plugin, replaced) => {
              upsert(plugin);
              if (!replaced) setSettingsFor(plugin.name);
            }}
          />

          <h2 className="mb-3 text-xl font-semibold">Installed plugins</h2>
          {data.plugins.length === 0 ? (
            <EmptyState />
          ) : (
            <PluginTable
              plugins={data.plugins}
              pendingToggles={pendingToggles}
              onToggle={(p, enabled) => void toggle(p, enabled)}
              onSettings={setSettingsFor}
              onUninstall={setUninstallFor}
            />
          )}

          {settingsPlugin && (
            <PluginSettingsModal
              key={settingsPlugin.name}
              plugin={settingsPlugin}
              limits={data.pluginLimits}
              contentTypeIds={data.contentTypes.map((t) => t.apiId)}
              onClose={() => setSettingsFor(null)}
              onSaved={upsert}
            />
          )}
          {uninstallFor && (
            <UninstallModal
              key={uninstallFor}
              name={uninstallFor}
              onClose={() => setUninstallFor(null)}
              onUninstalled={remove}
            />
          )}
        </>
      )}
    </>
  );
}

function EmptyState() {
  return (
    <div className="border-base-300 rounded-box border border-dashed p-8 text-center">
      <h3 className="mb-2 text-lg font-semibold">No plugins installed</h3>
      <p className="text-base-content/70 mx-auto max-w-xl text-sm">
        Plugins are sandboxed WebAssembly modules that hook into content writes: a <code>before_write</code> hook can
        check or reject an entry, and an <code>after_write</code> hook can react to it (for example to notify another
        service). Upload a module above to get started.
      </p>
      <p className="text-base-content/70 mt-3 text-sm">
        Sample plugins and build instructions live in the{" "}
        <code className="bg-base-200 rounded px-1 font-mono">examples/plugins</code> directory of the CMS repository.
      </p>
    </div>
  );
}
