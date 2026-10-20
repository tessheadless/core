// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useId, useState, type FormEvent } from "react";
import {
  FIELD_KINDS,
  TEXT_FORMATS,
  createComponent,
  createContentType,
  defaultPlural,
  hasErrors,
  toComponentInput,
  toInput,
  updateComponent,
  updateContentType,
  validateComponentInput,
  validateInput,
  type ComponentDefinition,
  type ContentType,
  type ContentTypeFieldInput,
  type FieldKind,
  type ModelKind,
  type ValidationErrors,
} from "../lib/contentTypes";
import { describeError } from "../lib/graphql";
import { ButtonSpinner, TextField } from "./Field";
import { Modal } from "./Modal";

type Model = ContentType | ComponentDefinition;

interface ModelEditorProps {
  kind: ModelKind;
  /** The model to edit, or null to create one. */
  model: Model | null;
  types: ContentType[];
  components: ComponentDefinition[];
  /** Built-in models are shown read-only. */
  readOnly?: boolean;
  onClose: () => void;
  onSaved: (saved: Model, created: boolean) => void;
}

/** Editor state: lists and numbers are edited as text. */
interface FieldDraft extends ContentTypeFieldInput {
  key: number;
  enumText: string;
  minText: string;
  maxText: string;
  unlimited: boolean;
  /** Present in the saved definition (removing it affects existing entries). */
  existing: boolean;
}

let nextKey = 1;

function toDraft(f: ContentTypeFieldInput, existing: boolean): FieldDraft {
  return {
    ...f,
    key: nextKey++,
    enumText: (f.enumValues ?? []).join(", "),
    minText: f.minLength != null ? String(f.minLength) : "",
    maxText: f.type === "TEXT" ? (f.maxLength != null ? String(f.maxLength) : "") : "",
    unlimited: f.type === "TEXT" && f.maxLength == null && existing,
    existing,
  };
}

function emptyField(): FieldDraft {
  return toDraft(
    {
      name: "",
      type: "TEXT",
      required: false,
      description: null,
      enumValues: null,
      relationTarget: null,
      many: false,
      minLength: null,
      maxLength: 80,
      pattern: null,
      component: null,
      repeatable: false,
      components: null,
    },
    false,
  );
}

function num(text: string): number | null {
  const n = Number.parseInt(text.trim(), 10);
  return Number.isFinite(n) && n >= 0 ? n : null;
}

function toFieldInput(d: FieldDraft): ContentTypeFieldInput {
  const text = d.type === "TEXT";
  return {
    name: d.name.trim(),
    type: d.type,
    required: d.required,
    description: d.description?.trim() ? d.description.trim() : null,
    enumValues:
      d.type === "ENUM"
        ? d.enumText
            .split(",")
            .map((v) => v.trim())
            .filter(Boolean)
        : null,
    relationTarget: d.type === "RELATION" ? d.relationTarget : null,
    many: d.type === "RELATION" ? d.many : false,
    minLength: text || d.type === "PASSWORD" ? num(d.minText) : null,
    maxLength: text ? (d.unlimited ? null : (num(d.maxText) ?? 80)) : null,
    pattern: text && d.pattern?.trim() ? d.pattern.trim() : null,
    component: d.type === "COMPONENT" ? d.component : null,
    repeatable: d.type === "COMPONENT" ? d.repeatable : false,
    components: d.type === "DYNAMIC_ZONE" ? (d.components ?? []) : null,
  };
}

export function ModelEditor({ kind, model, types, components, readOnly = false, onClose, onSaved }: ModelEditorProps) {
  const isComponent = kind === "component";
  const initial = model
    ? isComponent
      ? toComponentInput(model as ComponentDefinition)
      : toInput(model as ContentType)
    : null;
  const [apiId, setApiId] = useState(initial?.apiId ?? "");
  const [plural, setPlural] = useState<string>(model && !isComponent ? (model as ContentType).pluralApiId : "");
  const [description, setDescription] = useState(initial?.description ?? "");
  const [workflow, setWorkflow] = useState<boolean>(model && !isComponent ? (model as ContentType).workflow : false);
  const [fields, setFields] = useState<FieldDraft[]>(
    initial
      ? initial.fields.map((f) => toDraft(f, true))
      : [{ ...emptyField(), name: "title", required: !isComponent }],
  );
  const [errors, setErrors] = useState<ValidationErrors>({ fields: {} });
  const [serverError, setServerError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const descriptionId = useId();

  const removedExisting = initial
    ? initial.fields.filter((f) => !fields.some((d) => d.existing && d.name === f.name)).map((f) => f.name)
    : [];
  // Relation targets: content types (plus the one being created, for self-relations).
  const targets = [...new Set([...types.map((t) => t.apiId), ...(!isComponent && apiId ? [apiId] : [])])].sort();
  // Components usable here (a component cannot contain itself).
  const usableComponents = components.map((c) => c.apiId).filter((c) => !(isComponent && c === apiId));
  const kinds = FIELD_KINDS.filter((k) => !isComponent || k.inComponents);

  function update(key: number, patch: Partial<FieldDraft>) {
    setFields((fs) => fs.map((f) => (f.key === key ? { ...f, ...patch } : f)));
  }

  function move(index: number, delta: number) {
    setFields((fs) => {
      const next = [...fs];
      const [item] = next.splice(index, 1);
      next.splice(index + delta, 0, item);
      return next;
    });
  }

  async function submit(event: FormEvent, close: () => void) {
    event.preventDefault();
    if (readOnly) return;
    const fieldInputs = fields.map(toFieldInput);
    const base = {
      apiId: apiId.trim(),
      description: description.trim() ? description.trim() : null,
      fields: fieldInputs,
    };
    setServerError(null);
    try {
      if (isComponent) {
        const found = validateComponentInput(base, types, components, model?.apiId ?? null);
        setErrors(found);
        if (hasErrors(found)) return;
        setBusy(true);
        const saved = model ? await updateComponent(model.apiId, base) : await createComponent(base);
        onSaved(saved, !model);
      } else {
        const input = { ...base, pluralApiId: plural.trim() || defaultPlural(base.apiId), workflow };
        const found = validateInput(input, types, model?.apiId ?? null, components);
        setErrors(found);
        if (hasErrors(found)) return;
        setBusy(true);
        const saved = model ? await updateContentType(model.apiId, input) : await createContentType(input);
        onSaved(saved, !model);
      }
      close();
    } catch (e) {
      // Shown inline: toasts sit below the modal's top layer.
      setServerError(describeError(e));
      setBusy(false);
    }
  }

  const noun = isComponent ? "component" : "content model";
  const title = readOnly ? `${model?.apiId} (built-in)` : model ? `Edit ${model.apiId}` : `New ${noun}`;

  return (
    <Modal title={title} onClose={onClose} variant="drawer">
      {(close) => (
        <form onSubmit={(e) => void submit(e, close)} noValidate>
          {readOnly && (
            <div role="note" className="alert alert-info my-3 text-sm">
              This content type is managed by Tess (the media library) and cannot be changed. To attach media to your
              content, add a <strong>Relation</strong> field targeting <code>Asset</code> to your own models.
            </div>
          )}
          {isComponent && !readOnly && (
            <p className="text-base-content/70 my-2 text-sm">
              A component is a reusable group of fields. Add it to content types with a <strong>Component</strong> field
              (single or repeatable), offer it in <strong>dynamic zones</strong>, or nest it in other components.
              Changing it changes every model that uses it.
            </p>
          )}
          <fieldset disabled={readOnly || busy} className="contents">
            <div className={`grid gap-x-4 ${isComponent ? "" : "sm:grid-cols-2"}`}>
              <TextField
                label="API ID"
                value={apiId}
                onChange={setApiId}
                mono
                required
                disabled={Boolean(model) || readOnly || busy}
                error={errors.apiId}
                hint={
                  model
                    ? "Renaming is not supported."
                    : isComponent
                      ? "PascalCase, e.g. SeoMeta. Becomes a GraphQL type name."
                      : "PascalCase, e.g. BlogPost. Becomes the GraphQL type name."
                }
                autoComplete="off"
                spellCheck={false}
              />
              {!isComponent && (
                <TextField
                  label="Plural API ID"
                  value={plural}
                  onChange={setPlural}
                  mono
                  error={errors.pluralApiId}
                  placeholder={defaultPlural(apiId) || "blogPosts"}
                  hint="Name of the list query. Leave empty for the default."
                  autoComplete="off"
                  spellCheck={false}
                />
              )}
            </div>
            <fieldset className="fieldset">
              <label className="fieldset-label" htmlFor={descriptionId}>
                Description
              </label>
              <textarea
                id={descriptionId}
                className="textarea w-full"
                rows={2}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
              />
            </fieldset>
            {!isComponent && (
              <div className="my-2 flex items-start gap-3">
                <input
                  id={`${descriptionId}-workflow`}
                  type="checkbox"
                  className="toggle toggle-success mt-0.5"
                  checked={workflow}
                  onChange={(e) => setWorkflow(e.target.checked)}
                  aria-describedby={`${descriptionId}-workflow-hint`}
                />
                <div>
                  <label htmlFor={`${descriptionId}-workflow`} className="cursor-pointer font-medium">
                    Review workflow
                  </label>
                  <p id={`${descriptionId}-workflow-hint`} className="text-base-content/70 text-sm">
                    Contributors submit drafts; a reviewer or admin approves them before publication.
                  </p>
                </div>
              </div>
            )}

            <h3 className="mt-4 mb-2 font-semibold">Fields</h3>
            {errors.form && (
              <p role="alert" className="text-error mb-2 text-sm">
                {errors.form}
              </p>
            )}
            <ol className="space-y-3">
              {fields.map((f, i) => (
                <FieldRow
                  key={f.key}
                  field={f}
                  index={i}
                  count={fields.length}
                  kinds={kinds}
                  targets={targets}
                  components={usableComponents}
                  error={errors.fields[i]}
                  readOnly={readOnly}
                  onChange={(patch) => update(f.key, patch)}
                  onMove={(delta) => move(i, delta)}
                  onRemove={() => setFields((fs) => fs.filter((x) => x.key !== f.key))}
                />
              ))}
            </ol>
            {!readOnly && (
              <button
                type="button"
                className="btn btn-sm btn-outline mt-3"
                onClick={() => setFields((fs) => [...fs, emptyField()])}
              >
                + Add field
              </button>
            )}
          </fieldset>

          {model && !readOnly && removedExisting.length > 0 && (
            <div role="alert" className="alert alert-warning mt-4 text-sm">
              Removing {removedExisting.map((n) => `"${n}"`).join(", ")}: existing values are hidden and dropped the
              next time each entry is saved.
            </div>
          )}
          {model && !readOnly && (
            <p className="text-base-content/70 mt-3 text-xs">
              Changing a field&apos;s type or limits does not convert existing values: values that no longer fit are
              hidden or rejected on the next save.
            </p>
          )}
          {serverError && (
            <div role="alert" className="alert alert-error mt-4 text-sm">
              {serverError}
            </div>
          )}
          <div className="modal-action">
            <button type="button" className="btn" onClick={close}>
              {readOnly ? "Close" : "Cancel"}
            </button>
            {!readOnly && (
              <button type="submit" className="btn btn-primary" disabled={busy}>
                <ButtonSpinner busy={busy} />
                {model ? "Save changes" : `Create ${noun}`}
              </button>
            )}
          </div>
        </form>
      )}
    </Modal>
  );
}

interface FieldRowProps {
  field: FieldDraft;
  index: number;
  count: number;
  kinds: typeof FIELD_KINDS;
  targets: string[];
  components: string[];
  error?: string;
  readOnly: boolean;
  onChange: (patch: Partial<FieldDraft>) => void;
  onMove: (delta: number) => void;
  onRemove: () => void;
}

function FieldRow({
  field,
  index,
  count,
  kinds,
  targets,
  components,
  error,
  readOnly,
  onChange,
  onMove,
  onRemove,
}: FieldRowProps) {
  const id = useId();
  const kind = FIELD_KINDS.find((k) => k.kind === field.type);
  const format = TEXT_FORMATS.find((f) => f.pattern === field.pattern)?.label ?? (field.pattern ? "custom" : "");
  const zone = field.components ?? [];
  return (
    <li className={`rounded-box border p-3 ${error ? "border-error" : "border-base-300"}`}>
      <div className="flex flex-wrap items-end gap-2">
        <fieldset className="fieldset min-w-40 flex-1">
          <label className="fieldset-label" htmlFor={`${id}-name`}>
            Name
          </label>
          <input
            id={`${id}-name`}
            className="input input-sm w-full font-mono"
            value={field.name}
            onChange={(e) => onChange({ name: e.target.value })}
            placeholder="coverImage"
            autoComplete="off"
            spellCheck={false}
            aria-invalid={error ? true : undefined}
            aria-describedby={error ? `${id}-error` : undefined}
          />
        </fieldset>
        <fieldset className="fieldset min-w-40 flex-1">
          <label className="fieldset-label" htmlFor={`${id}-type`}>
            Type
          </label>
          <select
            id={`${id}-type`}
            className="select select-sm w-full"
            value={field.type}
            onChange={(e) => {
              const type = e.target.value as FieldKind;
              onChange({ type, minText: type === "PASSWORD" && !field.minText ? "8" : field.minText });
            }}
          >
            {kinds.map((k) => (
              <option key={k.kind} value={k.kind}>
                {k.label}
              </option>
            ))}
          </select>
        </fieldset>
        <label className="label mb-1 cursor-pointer gap-2 text-sm">
          <input
            type="checkbox"
            className="checkbox checkbox-sm"
            checked={field.required}
            onChange={(e) => onChange({ required: e.target.checked })}
          />
          Required
        </label>
        {!readOnly && (
          <div className="join mb-1">
            <button
              type="button"
              className="btn btn-sm join-item"
              onClick={() => onMove(-1)}
              disabled={index === 0}
              aria-label={`Move ${field.name || "field"} up`}
            >
              ↑
            </button>
            <button
              type="button"
              className="btn btn-sm join-item"
              onClick={() => onMove(1)}
              disabled={index === count - 1}
              aria-label={`Move ${field.name || "field"} down`}
            >
              ↓
            </button>
            <button
              type="button"
              className="btn btn-sm btn-error btn-outline join-item"
              onClick={onRemove}
              aria-label={`Remove ${field.name || "field"}`}
            >
              ✕
            </button>
          </div>
        )}
      </div>
      {kind && <p className="text-base-content/60 mt-1 text-xs">{kind.hint}</p>}

      {field.type === "TEXT" && (
        <div className="flex flex-wrap items-end gap-3">
          <fieldset className="fieldset w-28">
            <label className="fieldset-label" htmlFor={`${id}-min`}>
              Min length
            </label>
            <input
              id={`${id}-min`}
              type="number"
              min={0}
              className="input input-sm w-full"
              value={field.minText}
              onChange={(e) => onChange({ minText: e.target.value })}
              placeholder="0"
            />
          </fieldset>
          <fieldset className="fieldset w-28">
            <label className="fieldset-label" htmlFor={`${id}-max`}>
              Max length
            </label>
            <input
              id={`${id}-max`}
              type="number"
              min={1}
              className="input input-sm w-full"
              value={field.unlimited ? "" : field.maxText}
              onChange={(e) => onChange({ maxText: e.target.value })}
              placeholder={field.unlimited ? "∞" : "80"}
              disabled={field.unlimited}
            />
          </fieldset>
          <label className="label mb-1 cursor-pointer gap-2 text-sm">
            <input
              type="checkbox"
              className="checkbox checkbox-sm"
              checked={field.unlimited}
              onChange={(e) => onChange({ unlimited: e.target.checked })}
            />
            Unlimited
          </label>
          <fieldset className="fieldset min-w-40 flex-1">
            <label className="fieldset-label" htmlFor={`${id}-format`}>
              Format
            </label>
            <select
              id={`${id}-format`}
              className="select select-sm w-full"
              value={format}
              onChange={(e) => {
                const preset = TEXT_FORMATS.find((f) => f.label === e.target.value);
                onChange({
                  pattern: preset ? preset.pattern : e.target.value === "custom" ? (field.pattern ?? "") : null,
                });
              }}
            >
              <option value="">Any text</option>
              {TEXT_FORMATS.map((f) => (
                <option key={f.label} value={f.label}>
                  {f.label}
                </option>
              ))}
              <option value="custom">Custom regex…</option>
            </select>
          </fieldset>
          {format === "custom" && (
            <fieldset className="fieldset w-full">
              <label className="fieldset-label" htmlFor={`${id}-pattern`}>
                Regular expression (the whole value must match)
              </label>
              <input
                id={`${id}-pattern`}
                className="input input-sm w-full font-mono"
                value={field.pattern ?? ""}
                onChange={(e) => onChange({ pattern: e.target.value })}
                placeholder="[A-Z]{2}-[0-9]{4}"
                spellCheck={false}
              />
            </fieldset>
          )}
        </div>
      )}
      {field.type === "PASSWORD" && (
        <fieldset className="fieldset w-36">
          <label className="fieldset-label" htmlFor={`${id}-pwmin`}>
            Min length
          </label>
          <input
            id={`${id}-pwmin`}
            type="number"
            min={1}
            className="input input-sm w-full"
            value={field.minText}
            onChange={(e) => onChange({ minText: e.target.value })}
            placeholder="8"
          />
        </fieldset>
      )}
      {field.type === "ENUM" && (
        <fieldset className="fieldset">
          <label className="fieldset-label" htmlFor={`${id}-enum`}>
            Values (comma-separated)
          </label>
          <input
            id={`${id}-enum`}
            className="input input-sm w-full font-mono"
            value={field.enumText}
            onChange={(e) => onChange({ enumText: e.target.value })}
            placeholder="DRAFTED, LIVE, ARCHIVED"
            spellCheck={false}
          />
        </fieldset>
      )}
      {field.type === "RELATION" && (
        <div className="flex flex-wrap items-end gap-3">
          <fieldset className="fieldset min-w-48 flex-1">
            <label className="fieldset-label" htmlFor={`${id}-target`}>
              Related content type
            </label>
            <select
              id={`${id}-target`}
              className="select select-sm w-full"
              value={field.relationTarget ?? ""}
              onChange={(e) => onChange({ relationTarget: e.target.value || null })}
            >
              <option value="">Choose…</option>
              {targets.map((t) => (
                <option key={t} value={t}>
                  {t}
                  {t === "Asset" ? " (media)" : ""}
                </option>
              ))}
            </select>
          </fieldset>
          <label className="label mb-1 cursor-pointer gap-2 text-sm">
            <input
              type="checkbox"
              className="checkbox checkbox-sm"
              checked={field.many}
              onChange={(e) => onChange({ many: e.target.checked })}
            />
            Many
          </label>
        </div>
      )}
      {field.type === "COMPONENT" && (
        <div className="flex flex-wrap items-end gap-3">
          <fieldset className="fieldset min-w-48 flex-1">
            <label className="fieldset-label" htmlFor={`${id}-component`}>
              Component
            </label>
            <select
              id={`${id}-component`}
              className="select select-sm w-full"
              value={field.component ?? ""}
              onChange={(e) => onChange({ component: e.target.value || null })}
            >
              <option value="">Choose…</option>
              {components.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </select>
          </fieldset>
          <label className="label mb-1 cursor-pointer gap-2 text-sm">
            <input
              type="checkbox"
              className="checkbox checkbox-sm"
              checked={field.repeatable}
              onChange={(e) => onChange({ repeatable: e.target.checked })}
            />
            Repeatable
          </label>
          {components.length === 0 && (
            <p className="text-warning w-full text-xs">No components yet: create one on the Components page.</p>
          )}
        </div>
      )}
      {field.type === "DYNAMIC_ZONE" && (
        <fieldset className="fieldset">
          <legend className="fieldset-legend text-sm font-normal">Allowed components</legend>
          <div className="flex flex-wrap gap-x-4 gap-y-1">
            {components.map((c) => (
              <label key={c} className="label cursor-pointer gap-2 text-sm">
                <input
                  type="checkbox"
                  className="checkbox checkbox-sm"
                  checked={zone.includes(c)}
                  onChange={(e) =>
                    onChange({ components: e.target.checked ? [...zone, c] : zone.filter((x) => x !== c) })
                  }
                />
                <span className="font-mono">{c}</span>
              </label>
            ))}
            {components.length === 0 && (
              <p className="text-warning text-xs">No components yet: create some on the Components page.</p>
            )}
          </div>
        </fieldset>
      )}
      <fieldset className="fieldset">
        <label className="fieldset-label" htmlFor={`${id}-desc`}>
          Description (optional)
        </label>
        <input
          id={`${id}-desc`}
          className="input input-sm w-full"
          value={field.description ?? ""}
          onChange={(e) => onChange({ description: e.target.value })}
        />
      </fieldset>
      {error && (
        <p id={`${id}-error`} role="alert" className="text-error mt-1 text-sm">
          {error}
        </p>
      )}
    </li>
  );
}
