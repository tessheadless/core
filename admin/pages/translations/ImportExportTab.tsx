// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState, type FormEvent } from "react";
import { ButtonSpinner, TextField } from "../../components/Field";
import { useToast } from "../../components/Toasts";
import { describeError } from "../../lib/graphql";
import {
  downloadExport,
  FORMAT_LABELS,
  importTranslations,
  type ImportReport,
  type OverviewData,
  type TranslationFormat,
  type TranslationState,
} from "../../lib/i18n";
import { useViewer } from "../../lib/viewer";

interface Props {
  overview: OverviewData;
  onChanged: () => void;
}

/** Guesses the format from a file name. */
function detect(fileName: string): TranslationFormat | null {
  const n = fileName.toLowerCase();
  if (n.endsWith(".po") || n.endsWith(".pot")) return "PO";
  if (n.endsWith(".xliff") || n.endsWith(".xlf")) return "XLIFF";
  if (n.endsWith(".arb")) return "ARB";
  if (n.endsWith(".properties")) return "PROPERTIES";
  if (n.endsWith(".strings")) return "APPLE_STRINGS";
  if (n.endsWith(".csv")) return "CSV";
  if (n.endsWith(".xml")) return "ANDROID_XML";
  if (n.endsWith(".json")) return "JSON_NESTED";
  return null;
}

/** `fr.json`, `messages_fr_CA.properties`, `app_de.arb` → a language tag guess. */
function guessLanguage(fileName: string, tags: string[]): string | null {
  const stem = fileName.replace(/\.[^.]+$/, "").replace(/_/g, "-");
  const parts = stem.split(/[/.]/).pop() ?? stem;
  return tags.find((t) => parts.toLowerCase().endsWith(t.toLowerCase())) ?? null;
}

export function ImportExportTab({ overview, onChanged }: Props) {
  const { translations: can, isAdmin } = useViewer();
  const formats = overview.translationFormats;
  const tags = overview.translationLanguages.map((l) => l.tag);

  return (
    <div className="grid gap-8 lg:grid-cols-2">
      {can.canEdit ? (
        <ImportForm overview={overview} onChanged={onChanged} canReview={can.canReview} isAdmin={isAdmin} />
      ) : (
        <p className="text-base-content/70">Your role does not allow importing translations.</p>
      )}
      <ExportForm overview={overview} formats={formats.map((f) => f.format)} tags={tags} />
    </div>
  );
}

function ImportForm({ overview, onChanged, canReview, isAdmin }: Props & { canReview: boolean; isAdmin: boolean }) {
  const toast = useToast();
  const tags = overview.translationLanguages.map((l) => l.tag);
  const [file, setFile] = useState<File | null>(null);
  const [format, setFormat] = useState<TranslationFormat>("JSON_NESTED");
  const [language, setLanguage] = useState("");
  const [namespace, setNamespace] = useState("");
  const [mode, setMode] = useState<"KEEP" | "OVERRIDE">("KEEP");
  const [state, setState] = useState<TranslationState>("TRANSLATED");
  const [tagList, setTagList] = useState("");
  const [createLanguages, setCreateLanguages] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);
  const info = overview.translationFormats.find((f) => f.format === format);

  function pick(f: File | null) {
    setFile(f);
    setReport(null);
    if (!f) return;
    const detected = detect(f.name);
    if (detected) setFormat(detected);
    const lang = guessLanguage(f.name, tags);
    if (lang) setLanguage(lang);
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!file) return;
    setBusy(true);
    setError(null);
    try {
      const r = await importTranslations({
        format,
        content: await file.text(),
        language: info?.multiLanguage ? null : language || null,
        namespace: namespace.trim(),
        mode,
        state,
        tags: tagList
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
        createLanguages,
      });
      setReport(r);
      toast.success(`Imported ${r.translationsCreated + r.translationsUpdated} translations.`);
      onChanged();
    } catch (err) {
      setError(describeError(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form onSubmit={(e) => void submit(e)} className="card bg-base-200">
      <div className="card-body">
        <h2 className="card-title">Import</h2>
        <fieldset className="fieldset">
          <label className="fieldset-label" htmlFor="import-file">
            File
          </label>
          <input
            id="import-file"
            type="file"
            className="file-input w-full"
            onChange={(e) => pick(e.target.files?.[0] ?? null)}
            required
          />
        </fieldset>
        <div className="grid gap-2 sm:grid-cols-2">
          <fieldset className="fieldset">
            <label className="fieldset-label" htmlFor="import-format">
              Format
            </label>
            <select
              id="import-format"
              className="select w-full"
              value={format}
              onChange={(e) => setFormat(e.target.value as TranslationFormat)}
            >
              {overview.translationFormats
                .filter((f) => f.importable)
                .map((f) => (
                  <option key={f.format} value={f.format}>
                    {FORMAT_LABELS[f.format]}
                  </option>
                ))}
            </select>
          </fieldset>
          {!info?.multiLanguage && (
            <fieldset className="fieldset">
              <label className="fieldset-label" htmlFor="import-language">
                Language
              </label>
              <select
                id="import-language"
                className="select w-full"
                value={language}
                onChange={(e) => setLanguage(e.target.value)}
              >
                <option value="">Declared in the file</option>
                {overview.translationLanguages.map((l) => (
                  <option key={l.tag} value={l.tag}>
                    {l.tag} · {l.name}
                  </option>
                ))}
              </select>
            </fieldset>
          )}
        </div>
        <TextField
          label="Namespace"
          value={namespace}
          onChange={setNamespace}
          mono
          hint="Keys are imported into this namespace (empty = default)."
        />
        <div className="grid gap-2 sm:grid-cols-2">
          <fieldset className="fieldset">
            <legend className="fieldset-legend">Existing translations</legend>
            <label className="label cursor-pointer justify-start gap-2">
              <input
                type="radio"
                className="radio radio-sm"
                checked={mode === "KEEP"}
                onChange={() => setMode("KEEP")}
              />{" "}
              Keep
            </label>
            <label className="label cursor-pointer justify-start gap-2">
              <input
                type="radio"
                className="radio radio-sm"
                checked={mode === "OVERRIDE"}
                onChange={() => setMode("OVERRIDE")}
              />{" "}
              Override
            </label>
          </fieldset>
          <fieldset className="fieldset">
            <legend className="fieldset-legend">Imported texts are</legend>
            <label className="label cursor-pointer justify-start gap-2">
              <input
                type="radio"
                className="radio radio-sm"
                checked={state === "TRANSLATED"}
                onChange={() => setState("TRANSLATED")}
              />{" "}
              To review
            </label>
            <label className="label cursor-pointer justify-start gap-2">
              <input
                type="radio"
                className="radio radio-sm"
                checked={state === "REVIEWED"}
                onChange={() => setState("REVIEWED")}
                disabled={!canReview}
              />{" "}
              Reviewed
            </label>
          </fieldset>
        </div>
        <TextField
          label="Tags for new keys"
          value={tagList}
          onChange={setTagList}
          hint="Comma-separated, e.g. import-2026-10."
        />
        {isAdmin && (
          <label className="label cursor-pointer justify-start gap-2">
            <input
              type="checkbox"
              className="checkbox checkbox-sm"
              checked={createLanguages}
              onChange={(e) => setCreateLanguages(e.target.checked)}
            />
            Add languages found in the file
          </label>
        )}
        {error && <p className="text-error text-sm">{error}</p>}
        {report && (
          <output className="alert alert-success text-sm">
            <span>
              {report.keysCreated} keys created, {report.translationsCreated} translations added,{" "}
              {report.translationsUpdated} updated, {report.translationsSkipped} skipped ({report.languages.join(", ")}
              ).
            </span>
          </output>
        )}
        <button type="submit" className="btn btn-primary mt-2" disabled={busy || !file}>
          <ButtonSpinner busy={busy} />
          Import
        </button>
      </div>
    </form>
  );
}

function ExportForm({
  overview,
  formats,
  tags,
}: {
  overview: OverviewData;
  formats: TranslationFormat[];
  tags: string[];
}) {
  const toast = useToast();
  const [format, setFormat] = useState<TranslationFormat>("JSON_NESTED");
  const [languages, setLanguages] = useState<string[]>([]);
  const [namespaces, setNamespaces] = useState<string[]>([]);
  const [reviewedOnly, setReviewedOnly] = useState(false);
  const [fallback, setFallback] = useState(false);
  const [busy, setBusy] = useState(false);
  const slug = overview.translationFormats.find((f) => f.format === format)?.slug ?? "json";

  async function run() {
    setBusy(true);
    try {
      await downloadExport({
        format,
        languages,
        namespaces,
        states: reviewedOnly ? ["REVIEWED"] : ["TRANSLATED", "REVIEWED"],
        fallbackToBase: fallback,
      });
    } catch (e) {
      toast.error(describeError(e));
    } finally {
      setBusy(false);
    }
  }

  const toggle = (list: string[], set: (v: string[]) => void, v: string, on: boolean) =>
    set(on ? [...list, v] : list.filter((x) => x !== v));

  return (
    <div className="card bg-base-200">
      <div className="card-body">
        <h2 className="card-title">Export</h2>
        <fieldset className="fieldset">
          <label className="fieldset-label" htmlFor="export-format">
            Format
          </label>
          <select
            id="export-format"
            className="select w-full"
            value={format}
            onChange={(e) => setFormat(e.target.value as TranslationFormat)}
          >
            {formats.map((f) => (
              <option key={f} value={f}>
                {FORMAT_LABELS[f]}
              </option>
            ))}
          </select>
        </fieldset>
        <fieldset className="fieldset">
          <legend className="fieldset-legend">Languages (none = all)</legend>
          <div className="flex flex-wrap gap-3">
            {tags.map((t) => (
              <label key={t} className="label cursor-pointer gap-1">
                <input
                  type="checkbox"
                  className="checkbox checkbox-xs"
                  checked={languages.includes(t)}
                  onChange={(e) => toggle(languages, setLanguages, t, e.target.checked)}
                />
                <span className="font-mono text-sm">{t}</span>
              </label>
            ))}
          </div>
        </fieldset>
        {overview.translationNamespaces.length > 1 && (
          <fieldset className="fieldset">
            <legend className="fieldset-legend">Namespaces (none = all)</legend>
            <div className="flex flex-wrap gap-3">
              {overview.translationNamespaces.map((ns) => (
                <label key={ns} className="label cursor-pointer gap-1">
                  <input
                    type="checkbox"
                    className="checkbox checkbox-xs"
                    checked={namespaces.includes(ns)}
                    onChange={(e) => toggle(namespaces, setNamespaces, ns, e.target.checked)}
                  />
                  <span className="font-mono text-sm">{ns || "(default)"}</span>
                </label>
              ))}
            </div>
          </fieldset>
        )}
        <label className="label cursor-pointer justify-start gap-2">
          <input
            type="checkbox"
            className="checkbox checkbox-sm"
            checked={reviewedOnly}
            onChange={(e) => setReviewedOnly(e.target.checked)}
          />
          Reviewed translations only
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
        <button
          type="button"
          className="btn btn-primary mt-2"
          disabled={busy || tags.length === 0}
          onClick={() => void run()}
        >
          <ButtonSpinner busy={busy} />
          Download
        </button>
        <p className="text-base-content/70 mt-2 text-xs">
          Live files are also served at <code>/i18n/{slug}/…</code>, e.g.{" "}
          <code>
            /i18n/{slug}/{tags[0] ?? "en"}
            {slug === "i18next" ? "/translation.json" : ".json"}
          </code>
          .
        </p>
      </div>
    </div>
  );
}
