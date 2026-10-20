// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useId, useRef, useState, type FormEvent } from "react";
import { useToast } from "../../components/Toasts";
import { formatBytes } from "../../lib/format";
import { describeError } from "../../lib/graphql";
import {
  looksLikeWasm,
  PLUGIN_NAME_PATTERN,
  suggestName,
  uploadPlugin,
  type Plugin,
  type PluginLimits,
} from "../../lib/plugins";

interface InstallFormProps {
  limits: PluginLimits;
  existingNames: string[];
  onInstalled: (plugin: Plugin, replaced: boolean) => void;
}

export function InstallForm({ limits, existingNames, onInstalled }: InstallFormProps) {
  const toast = useToast();
  const ids = { file: useId(), name: useId(), nameHelp: useId(), fileHelp: useId() };
  const fileInput = useRef<HTMLInputElement>(null);
  const [file, setFile] = useState<File | null>(null);
  const [name, setName] = useState("");
  const [nameTouched, setNameTouched] = useState(false);
  const [fileError, setFileError] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);

  const nameValid = PLUGIN_NAME_PATTERN.test(name);
  const showNameError = nameTouched && name !== "" && !nameValid;
  const replacing = nameValid && existingNames.includes(name);

  async function pickFile(selected: File | null) {
    setFile(selected);
    setFileError(null);
    if (!selected) return;
    if (!nameTouched || name === "") setName(suggestName(selected.name));
    if (selected.size > limits.maxModuleBytes) {
      setFileError(`The module is ${formatBytes(selected.size)}; the limit is ${formatBytes(limits.maxModuleBytes)}.`);
    } else if (!(await looksLikeWasm(selected))) {
      setFileError("This file is not a WebAssembly module (missing the \\0asm header).");
    }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    setNameTouched(true);
    if (!file || fileError || !nameValid) return;
    setUploading(true);
    try {
      const plugin = await uploadPlugin(name, file);
      onInstalled(plugin, replacing);
      toast.success(
        replacing
          ? `Plugin "${plugin.name}" replaced.`
          : `Plugin "${plugin.name}" installed (disabled). Review its settings, then enable it.`,
      );
      setFile(null);
      setName("");
      setNameTouched(false);
      if (fileInput.current) fileInput.current.value = "";
    } catch (e) {
      toast.error(`Upload failed: ${describeError(e)}`);
    } finally {
      setUploading(false);
    }
  }

  return (
    <section className="card bg-base-100 border-base-300 mb-8 border" aria-labelledby="install-title">
      <form className="card-body" onSubmit={(e) => void submit(e)} noValidate>
        <h2 id="install-title" className="card-title">
          Install or replace a plugin
        </h2>
        <p className="text-base-content/70 text-sm">
          Upload a compiled <code>.wasm</code> module. New plugins are installed <strong>disabled</strong>: review their
          settings (content types, network access, limits) and enable them once you trust them. Uploading with the name
          of an existing plugin replaces its module.
        </p>
        <div className="grid gap-4 md:grid-cols-2">
          <fieldset className="fieldset">
            <label className="fieldset-label" htmlFor={ids.file}>
              WebAssembly module
            </label>
            <input
              ref={fileInput}
              id={ids.file}
              type="file"
              accept=".wasm,application/wasm"
              className={`file-input w-full ${fileError ? "file-input-error" : ""}`}
              onChange={(e) => void pickFile(e.target.files?.[0] ?? null)}
              disabled={uploading}
              aria-invalid={fileError ? true : undefined}
              aria-describedby={ids.fileHelp}
            />
            <p id={ids.fileHelp} className={`text-xs ${fileError ? "text-error" : "text-base-content/60"}`}>
              {fileError ??
                (file
                  ? `${file.name} — ${formatBytes(file.size)}`
                  : `Maximum size: ${formatBytes(limits.maxModuleBytes)}.`)}
            </p>
          </fieldset>
          <fieldset className="fieldset">
            <label className="fieldset-label" htmlFor={ids.name}>
              Plugin name
            </label>
            <input
              id={ids.name}
              type="text"
              className={`input w-full font-mono ${showNameError ? "input-error" : ""}`}
              placeholder="my-plugin"
              value={name}
              maxLength={48}
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setName(e.target.value)}
              onBlur={() => setNameTouched(true)}
              disabled={uploading}
              aria-invalid={showNameError ? true : undefined}
              aria-describedby={ids.nameHelp}
            />
            <p id={ids.nameHelp} className={`text-xs ${showNameError ? "text-error" : "text-base-content/60"}`}>
              2–48 characters: lowercase letters, digits and dashes, starting with a letter.
            </p>
          </fieldset>
        </div>
        {replacing && (
          <output className="alert alert-warning alert-soft text-sm">
            A plugin named <code className="font-mono">{name}</code> already exists: its module will be replaced.
          </output>
        )}
        {uploading && <progress className="progress progress-primary w-full" aria-label="Uploading plugin" />}
        <div className="card-actions justify-end">
          <button type="submit" className="btn btn-primary" disabled={uploading || !file || !!fileError || !nameValid}>
            {uploading && <span className="loading loading-spinner loading-sm" aria-hidden="true" />}
            {uploading ? "Uploading…" : replacing ? "Replace plugin" : "Install plugin"}
          </button>
        </div>
      </form>
    </section>
  );
}
