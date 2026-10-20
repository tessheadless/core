// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState } from "react";
import { ButtonSpinner } from "../../components/Field";
import { useToast } from "../../components/Toasts";
import { describeError } from "../../lib/graphql";
import { FORMAT_LABELS, liveUrl, updateSettings, type OverviewData } from "../../lib/i18n";

/** Live files at /i18n/{format}/{file}: who may read them and what they contain. */
export function ServingTab({ overview, onChanged }: { overview: OverviewData; onChanged: () => void }) {
  const toast = useToast();
  const s = overview.translationSettings;
  const [isPublic, setPublic] = useState(s.public);
  const [reviewedOnly, setReviewedOnly] = useState(s.servedStates.length === 1 && s.servedStates[0] === "REVIEWED");
  const [fallback, setFallback] = useState(s.fallbackToBase);
  const [busy, setBusy] = useState(false);
  const lang = overview.translationLanguages[0]?.tag ?? "en";

  async function save() {
    setBusy(true);
    try {
      await updateSettings({
        public: isPublic,
        servedStates: reviewedOnly ? ["REVIEWED"] : ["TRANSLATED", "REVIEWED"],
        fallbackToBase: fallback,
      });
      toast.success("Serving settings saved.");
      onChanged();
    } catch (e) {
      toast.error(describeError(e));
    } finally {
      setBusy(false);
    }
  }

  const example = (slug: string) => {
    if (slug === "i18next") return `${lang}/translation.json`;
    if (slug === "android") return `values-${lang}/strings.xml`;
    if (slug === "apple") return `${lang}.lproj/Localizable.strings`;
    if (slug === "csv") return "translations.csv";
    if (slug === "arb") return `app_${lang.replace("-", "_")}.arb`;
    if (slug === "properties") return `messages_${lang.replace("-", "_")}.properties`;
    const ext: Record<string, string> = { xliff: "xliff", po: "po", yaml: "yml" };
    return `${lang}.${ext[slug] ?? "json"}`;
  };

  return (
    <section aria-label="Serving" className="grid gap-8 lg:grid-cols-2">
      <div className="card bg-base-200 h-fit">
        <div className="card-body">
          <h2 className="card-title">Live files</h2>
          <p className="text-base-content/70 text-sm">
            Tess serves the current translations over HTTP (ETag, cached 60 s), handy for development or small sites.
            For production traffic, publish to a delivery target behind a CDN.
          </p>
          <label className="label cursor-pointer justify-start gap-2">
            <input
              type="checkbox"
              className="toggle toggle-sm"
              checked={isPublic}
              onChange={(e) => setPublic(e.target.checked)}
            />
            Public (no credentials needed; also requires TESS_PUBLIC_READ)
          </label>
          <label className="label cursor-pointer justify-start gap-2">
            <input
              type="checkbox"
              className="checkbox checkbox-sm"
              checked={reviewedOnly}
              onChange={(e) => setReviewedOnly(e.target.checked)}
            />
            Serve reviewed translations only
          </label>
          <label className="label cursor-pointer justify-start gap-2">
            <input
              type="checkbox"
              className="checkbox checkbox-sm"
              checked={fallback}
              onChange={(e) => setFallback(e.target.checked)}
            />
            Fill missing texts with the base language
          </label>
          <button type="button" className="btn btn-primary mt-2 w-fit" disabled={busy} onClick={() => void save()}>
            <ButtonSpinner busy={busy} />
            Save
          </button>
        </div>
      </div>
      <div className="overflow-x-auto">
        <table className="table-sm table">
          <thead>
            <tr>
              <th>Format</th>
              <th>Example URL</th>
            </tr>
          </thead>
          <tbody>
            {overview.translationFormats.map((f) => {
              const url = liveUrl(f.slug, example(f.slug));
              return (
                <tr key={f.format}>
                  <td className="whitespace-nowrap">{FORMAT_LABELS[f.format]}</td>
                  <td>
                    <a
                      className="link link-hover font-mono text-xs break-all"
                      href={url}
                      target="_blank"
                      rel="noreferrer"
                    >
                      {url}
                    </a>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}
