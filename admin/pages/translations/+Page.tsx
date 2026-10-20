// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useEffect, useState } from "react";
import { fetchOverview } from "../../lib/i18n";
import { useQuery } from "../../lib/useQuery";
import { useViewer } from "../../lib/viewer";
import { DeliveryTab } from "./DeliveryTab";
import { ImportExportTab } from "./ImportExportTab";
import { KeysTab } from "./KeysTab";
import { LanguagesTab } from "./LanguagesTab";
import { ServingTab } from "./ServingTab";

type Tab = "keys" | "languages" | "files" | "delivery" | "serving";

const TABS: { id: Tab; label: string; admin?: boolean }[] = [
  { id: "keys", label: "Keys" },
  { id: "languages", label: "Languages" },
  { id: "files", label: "Import & export" },
  { id: "delivery", label: "Delivery", admin: true },
  { id: "serving", label: "Serving", admin: true },
];

function tabFromHash(): Tab {
  const h = typeof window === "undefined" ? "" : window.location.hash.slice(1);
  return TABS.some((t) => t.id === h) ? (h as Tab) : "keys";
}

export default function Page() {
  const { isAdmin } = useViewer();
  const overview = useQuery(fetchOverview);
  const [tab, setTab] = useState<Tab>(tabFromHash);

  useEffect(() => {
    const onHash = () => setTab(tabFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  const data = overview.data;
  const tabs = TABS.filter((t) => !t.admin || isAdmin);

  return (
    <>
      <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-3xl font-bold">Translations</h1>
        {data && data.translationLanguages.length > 0 && (
          <div className="flex flex-wrap gap-3" aria-label="Translation progress">
            {data.translationStats.map((s) => {
              const pct = s.total === 0 ? 100 : Math.round((s.translated / s.total) * 100);
              return (
                <div key={s.language} className="flex items-center gap-2 text-sm" title={`${s.reviewed} reviewed`}>
                  <span className="font-mono">{s.language}</span>
                  <progress
                    className="progress progress-success w-20"
                    value={pct}
                    max={100}
                    aria-label={`${s.language} ${pct}% translated`}
                  />
                  <span className="text-base-content/70">{pct}%</span>
                </div>
              );
            })}
          </div>
        )}
      </div>

      <div role="tablist" className="tabs tabs-border mb-6">
        {tabs.map((t) => (
          <a
            key={t.id}
            role="tab"
            href={`#${t.id}`}
            className={`tab ${tab === t.id ? "tab-active" : ""}`}
            aria-selected={tab === t.id}
          >
            {t.label}
          </a>
        ))}
      </div>

      {overview.error && (
        <div role="alert" className="alert alert-error mb-4">
          <span>Could not load translations: {overview.error}</span>
          <button type="button" className="btn btn-sm" onClick={overview.reload}>
            Retry
          </button>
        </div>
      )}
      {!data && overview.loading && (
        <div className="flex justify-center py-12">
          <span className="loading loading-spinner loading-lg" aria-label="Loading translations" />
        </div>
      )}

      {data && data.translationLanguages.length === 0 && tab !== "languages" && (
        <output className="alert mb-6">
          <span>
            No language yet. {isAdmin ? "Add one in the Languages tab to start." : "Ask an Admin to add languages."}
          </span>
          {isAdmin && (
            <a href="#languages" className="btn btn-sm btn-primary">
              Add a language
            </a>
          )}
        </output>
      )}

      {data && tab === "keys" && <KeysTab overview={data} onChanged={overview.reload} />}
      {data && tab === "languages" && <LanguagesTab overview={data} onChanged={overview.reload} />}
      {data && tab === "files" && <ImportExportTab overview={data} onChanged={overview.reload} />}
      {data && tab === "delivery" && isAdmin && <DeliveryTab overview={data} />}
      {data && tab === "serving" && isAdmin && <ServingTab overview={data} onChanged={overview.reload} />}
    </>
  );
}
