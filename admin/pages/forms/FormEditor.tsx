// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState, type FormEvent } from "react";
import { ButtonSpinner, TextField } from "../../components/Field";
import { Modal } from "../../components/Modal";
import { FIELD_TYPES, saveForm, type Form, type FormField, type FormFieldType } from "../../lib/forms";
import { describeError } from "../../lib/graphql";

interface Props {
  form: Form | null;
  onClose: () => void;
  onSaved: (f: Form) => void;
}

const newField = (n: number): FormField => ({
  name: `field${n}`,
  label: "",
  type: "TEXT",
  required: false,
  help: null,
  placeholder: null,
  options: null,
  maxLength: null,
  min: null,
  max: null,
});

/** `My field` → `myField`. */
function toName(label: string): string {
  const words = label
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^A-Za-z0-9]+/g, " ")
    .trim()
    .split(" ")
    .filter(Boolean);
  const name = words
    .map((w, i) => (i === 0 ? w.toLowerCase() : w[0].toUpperCase() + w.slice(1).toLowerCase()))
    .join("");
  return /^[A-Za-z]/.test(name) ? name : `field${name}`;
}

/** Whether a field's name is still derived from its label. */
function follows(f: FormField, form: Form | null): boolean {
  return !form?.fields.some((x) => x.name === f.name) && (/^field\d+$/.test(f.name) || f.name === toName(f.label));
}

const hasOptions = (t: FormFieldType) => t === "SELECT" || t === "MULTI_SELECT";

export function FormEditor({ form, onClose, onSaved }: Props) {
  const [title, setTitle] = useState(form?.title ?? "");
  const [slug, setSlug] = useState(form?.slug ?? "");
  const [description, setDescription] = useState(form?.description ?? "");
  const [enabled, setEnabled] = useState(form?.enabled ?? true);
  const [successMessage, setSuccessMessage] = useState(form?.successMessage ?? "");
  const [redirectUrl, setRedirectUrl] = useState(form?.redirectUrl ?? "");
  const [fields, setFields] = useState<FormField[]>(
    form?.fields ?? [{ ...newField(1), name: "email", label: "Email", type: "EMAIL", required: true }],
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const update = (i: number, patch: Partial<FormField>) =>
    setFields((fs) => fs.map((f, j) => (j === i ? { ...f, ...patch } : f)));
  const move = (i: number, by: number) =>
    setFields((fs) => {
      const next = [...fs];
      const [f] = next.splice(i, 1);
      next.splice(i + by, 0, f);
      return next;
    });

  async function submit(e: FormEvent, close: () => void) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const saved = await saveForm(form?.id ?? null, {
        title: title.trim(),
        slug: slug.trim(),
        description: description.trim() || null,
        enabled,
        successMessage: successMessage.trim() || null,
        redirectUrl: redirectUrl.trim() || null,
        fields: fields.map((f) => ({
          ...f,
          options: hasOptions(f.type) ? (f.options ?? []).filter((o) => o.trim()) : null,
        })),
      });
      close();
      onSaved(saved);
    } catch (err) {
      setError(describeError(err));
      setBusy(false);
    }
  }

  return (
    <Modal title={form ? `Edit “${form.title}”` : "New form"} onClose={onClose} wide>
      {(close) => (
        <form onSubmit={(e) => void submit(e, close)} className="mt-2">
          <div className="grid gap-x-4 sm:grid-cols-2">
            <TextField
              label="Title"
              value={title}
              onChange={(v) => {
                setTitle(v);
                if (!form && (slug === "" || slug === toSlug(title))) setSlug(toSlug(v));
              }}
              required
            />
            <TextField
              label="Slug"
              value={slug}
              onChange={setSlug}
              mono
              required
              hint="In the URL: /api/forms/<slug>"
            />
          </div>
          <TextField label="Description" value={description} onChange={setDescription} />

          <h3 className="mt-4 mb-2 font-semibold">Fields</h3>
          <ol className="flex flex-col gap-3">
            {fields.map((f, i) => (
              <li key={i} className="border-base-300 rounded-box border p-3">
                <div className="grid gap-x-3 sm:grid-cols-[2fr_1.5fr_1.5fr]">
                  <TextField
                    label="Label"
                    value={f.label}
                    // The name follows the label until edited by hand (not on saved forms:
                    // renaming a field orphans the values already submitted).
                    onChange={(v) => update(i, follows(f, form) ? { label: v, name: toName(v) } : { label: v })}
                    required
                  />
                  <TextField
                    label="Name"
                    value={f.name}
                    onChange={(v) => update(i, { name: v })}
                    mono
                    required
                    hint="Key in submissions"
                  />
                  <fieldset className="fieldset">
                    <label className="fieldset-label" htmlFor={`type-${i}`}>
                      Type
                    </label>
                    <select
                      id={`type-${i}`}
                      className="select w-full"
                      value={f.type}
                      onChange={(e) => update(i, { type: e.target.value as FormFieldType })}
                    >
                      {FIELD_TYPES.map((t) => (
                        <option key={t.value} value={t.value}>
                          {t.label}
                        </option>
                      ))}
                    </select>
                  </fieldset>
                </div>
                {hasOptions(f.type) && (
                  <fieldset className="fieldset">
                    <label className="fieldset-label" htmlFor={`options-${i}`}>
                      Options (one per line)
                    </label>
                    <textarea
                      id={`options-${i}`}
                      className="textarea w-full"
                      rows={3}
                      value={(f.options ?? []).join("\n")}
                      onChange={(e) => update(i, { options: e.target.value.split("\n") })}
                    />
                  </fieldset>
                )}
                {f.type === "NUMBER" && (
                  <div className="grid grid-cols-2 gap-x-3">
                    <TextField
                      label="Minimum"
                      value={f.min?.toString() ?? ""}
                      onChange={(v) => update(i, { min: v === "" ? null : Number(v) })}
                      type="number"
                    />
                    <TextField
                      label="Maximum"
                      value={f.max?.toString() ?? ""}
                      onChange={(v) => update(i, { max: v === "" ? null : Number(v) })}
                      type="number"
                    />
                  </div>
                )}
                <TextField label="Help text" value={f.help ?? ""} onChange={(v) => update(i, { help: v || null })} />
                <div className="mt-2 flex flex-wrap items-center gap-3">
                  <label className="flex cursor-pointer items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      className="checkbox checkbox-sm"
                      checked={f.required}
                      onChange={(e) => update(i, { required: e.target.checked })}
                    />
                    Required
                  </label>
                  <div className="ml-auto flex gap-1">
                    <button
                      type="button"
                      className="btn btn-xs btn-ghost"
                      disabled={i === 0}
                      onClick={() => move(i, -1)}
                      aria-label={`Move ${f.label || f.name} up`}
                    >
                      ↑
                    </button>
                    <button
                      type="button"
                      className="btn btn-xs btn-ghost"
                      disabled={i === fields.length - 1}
                      onClick={() => move(i, 1)}
                      aria-label={`Move ${f.label || f.name} down`}
                    >
                      ↓
                    </button>
                    <button
                      type="button"
                      className="btn btn-xs btn-ghost text-error"
                      disabled={fields.length === 1}
                      onClick={() => setFields((fs) => fs.filter((_, j) => j !== i))}
                    >
                      Remove
                    </button>
                  </div>
                </div>
              </li>
            ))}
          </ol>
          <button
            type="button"
            className="btn btn-sm btn-outline mt-3"
            onClick={() => setFields((fs) => [...fs, newField(fs.length + 1)])}
          >
            Add a field
          </button>

          <h3 className="mt-4 mb-2 font-semibold">After a submission</h3>
          <TextField
            label="Success message"
            value={successMessage}
            onChange={setSuccessMessage}
            hint="Returned to your front-end."
          />
          <TextField
            label="Redirect URL"
            value={redirectUrl}
            onChange={setRedirectUrl}
            hint="Plain HTML forms (no JavaScript) are sent back here, e.g. https://example.com/thanks."
          />
          <label className="mt-3 flex cursor-pointer items-center gap-2">
            <input
              type="checkbox"
              className="toggle toggle-success"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
            />
            Accept submissions
          </label>
          {error && <p className="text-error mt-3 text-sm">{error}</p>}
          <div className="modal-action">
            <button type="button" className="btn btn-ghost" onClick={close}>
              Cancel
            </button>
            <button type="submit" className="btn btn-primary" disabled={busy}>
              <ButtonSpinner busy={busy} />
              {form ? "Save" : "Create form"}
            </button>
          </div>
        </form>
      )}
    </Modal>
  );
}

function toSlug(title: string): string {
  return title
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}
