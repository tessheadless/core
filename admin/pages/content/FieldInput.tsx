// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import type { ContentTypeField } from "../../lib/contentTypes";
import {
  newComponentValue,
  relationOptions,
  uploadAsset,
  type FormValues,
  type Models,
  type RelationOption,
} from "../../lib/content";
import { MarkdownEditor } from "../../components/MarkdownEditor";
import { describeError } from "../../lib/graphql";

interface FieldInputProps {
  field: ContentTypeField;
  value: unknown;
  onChange: (value: unknown) => void;
  models: Models;
  readOnly: boolean;
  /** Editing an existing entry (passwords: empty = unchanged). */
  existing: boolean;
}

/** Labelled control for one field, recursive through components and dynamic zones. */
export function FieldInput(props: FieldInputProps) {
  const { field } = props;
  const id = useId();
  const nested = field.type === "COMPONENT" || field.type === "DYNAMIC_ZONE";
  const label = (
    <span className="flex items-center gap-2">
      <span className="font-mono">{field.name}</span>
      {field.required && <span className="text-error">*</span>}
      <span className="badge badge-ghost badge-xs">{kindLabel(field)}</span>
    </span>
  );
  const hint = [field.description, hintFor(field, props.existing)].filter(Boolean).join(" · ");
  return (
    <fieldset className={nested ? "fieldset border-base-300 rounded-box border p-3" : "fieldset"}>
      {nested ? (
        <legend className="fieldset-legend">{label}</legend>
      ) : (
        <label className="fieldset-label" htmlFor={id}>
          {label}
        </label>
      )}
      <Control {...props} id={id} />
      {hint && <p className="text-base-content/70 text-xs">{hint}</p>}
    </fieldset>
  );
}

function kindLabel(f: ContentTypeField): string {
  switch (f.type) {
    case "RELATION":
      return `→ ${f.relationTarget}${f.many ? " (many)" : ""}`;
    case "COMPONENT":
      return `${f.component}${f.repeatable ? " (repeatable)" : ""}`;
    case "DYNAMIC_ZONE":
      return "dynamic zone";
    default:
      return f.type.toLowerCase().replace("_", " ");
  }
}

function hintFor(f: ContentTypeField, existing: boolean): string | null {
  switch (f.type) {
    case "MARKDOWN":
      return "Markdown (MDX components allowed); raw HTML is rejected.";
    case "UUID":
      return existing ? null : "Generated automatically when left empty.";
    case "PASSWORD":
      return existing
        ? `Stored hashed. Leave empty to keep the current one (at least ${f.minLength ?? 8} characters).`
        : `Stored hashed (at least ${f.minLength ?? 8} characters).`;
    case "TEXT":
      return f.pattern ? `Format: ${f.pattern}` : null;
    case "JSON":
      return "Any JSON value.";
    default:
      return null;
  }
}

function Control({ field, value, onChange, models, readOnly, existing, id }: FieldInputProps & { id: string }) {
  const str = typeof value === "string" ? value : "";
  const common = { id, disabled: readOnly, required: field.required && field.type !== "PASSWORD" };
  switch (field.type) {
    case "TEXT": {
      const max = field.maxLength;
      const long = max === null || max > 200;
      const counter = (
        <span className={`text-xs ${max !== null && str.length > max ? "text-error" : "text-base-content/60"}`}>
          {str.length}
          {max !== null && ` / ${max}`}
        </span>
      );
      return (
        <div className="flex flex-col gap-1">
          {long ? (
            <textarea
              {...common}
              className="textarea w-full"
              rows={4}
              value={str}
              minLength={field.minLength ?? undefined}
              maxLength={max ?? undefined}
              onChange={(e) => onChange(e.target.value)}
            />
          ) : (
            <input
              {...common}
              className="input w-full"
              value={str}
              minLength={field.minLength ?? undefined}
              maxLength={max ?? undefined}
              onChange={(e) => onChange(e.target.value)}
            />
          )}
          <div className="text-right">{counter}</div>
        </div>
      );
    }
    case "MARKDOWN":
      return <MarkdownEditor {...common} value={str} onChange={onChange} />;
    case "INT":
    case "FLOAT":
      return (
        <input
          {...common}
          type="number"
          step={field.type === "INT" ? 1 : "any"}
          className="input w-full"
          value={str}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "BOOLEAN":
      return (
        <input
          id={id}
          type="checkbox"
          className="toggle"
          disabled={readOnly}
          checked={value === true}
          onChange={(e) => onChange(e.target.checked)}
        />
      );
    case "DATE":
      return <input {...common} type="date" className="input" value={str} onChange={(e) => onChange(e.target.value)} />;
    case "DATETIME":
      return (
        <input
          {...common}
          type="datetime-local"
          className="input"
          value={str}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "UUID":
      return (
        <input
          {...common}
          required={false}
          className="input w-full font-mono"
          value={str}
          placeholder={existing ? undefined : "auto"}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "PASSWORD":
      return (
        <input
          {...common}
          required={field.required && !existing}
          type="password"
          autoComplete="new-password"
          className="input w-full"
          value={str}
          minLength={field.minLength ?? undefined}
          placeholder={existing ? "unchanged" : undefined}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "JSON":
      return (
        <textarea
          {...common}
          className="textarea w-full font-mono text-sm"
          rows={5}
          value={str}
          spellCheck={false}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "ENUM":
      return (
        <select {...common} className="select w-full" value={str} onChange={(e) => onChange(e.target.value)}>
          <option value="">—</option>
          {(field.enumValues ?? []).map((v) => (
            <option key={v} value={v}>
              {v}
            </option>
          ))}
        </select>
      );
    case "RELATION":
      return (
        <RelationInput id={id} field={field} value={value} onChange={onChange} models={models} readOnly={readOnly} />
      );
    case "COMPONENT":
      return <ComponentInput field={field} value={value} onChange={onChange} models={models} readOnly={readOnly} />;
    case "DYNAMIC_ZONE":
      return <ZoneInput field={field} value={value} onChange={onChange} models={models} readOnly={readOnly} />;
  }
}

// ---- relations ------------------------------------------------------------------

function RelationInput({
  id,
  field,
  value,
  onChange,
  models,
  readOnly,
}: {
  id: string;
  field: ContentTypeField;
  value: unknown;
  onChange: (v: unknown) => void;
  models: Models;
  readOnly: boolean;
}) {
  const target = field.relationTarget ?? "";
  const [options, setOptions] = useState<RelationOption[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let cancelled = false;
    relationOptions(target, models).then(
      (o) => !cancelled && setOptions(o),
      (e: unknown) => !cancelled && setError(describeError(e)),
    );
    return () => {
      cancelled = true;
    };
  }, [target, models]);

  const selected: RelationOption[] = field.many
    ? ((value as RelationOption[] | null) ?? [])
    : value
      ? [value as RelationOption]
      : [];
  // Ids are re-encoded on every read: entries are matched by their stable `ref`.
  const byRef = new Map((options ?? []).map((o) => [o.ref, o]));
  const name = (r: RelationOption) => r.label || byRef.get(r.ref)?.label || r.ref;
  const selectedRefs = new Set(selected.map((r) => r.ref));
  const available = (options ?? []).filter((o) => !selectedRefs.has(o.ref));
  const current = field.many ? null : ((value as RelationOption | null) ?? null);

  async function upload(file: File) {
    setUploading(true);
    setError(null);
    try {
      const asset = await uploadAsset(file);
      setOptions((o) => [asset, ...(o ?? [])]);
      onChange(field.many ? [...selected, asset] : asset);
    } catch (e) {
      setError(describeError(e));
    } finally {
      setUploading(false);
    }
  }

  return (
    <div className="flex flex-col gap-2">
      {field.many && selected.length > 0 && (
        <ul className="flex flex-wrap gap-2">
          {selected.map((s, i) => (
            <li key={s.ref} className="badge badge-outline gap-1 py-3">
              {name(s)}
              {!readOnly && (
                <button
                  type="button"
                  className="btn btn-ghost btn-xs btn-circle"
                  aria-label={`Remove ${name(s)}`}
                  onClick={() => onChange(selected.filter((_, j) => j !== i))}
                >
                  ✕
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
      <div className="flex flex-wrap items-center gap-2">
        {!options && !error && <span className="loading loading-spinner loading-sm" aria-label="Loading entries" />}
        {options && (
          <select
            id={id}
            className="select min-w-0 flex-1"
            disabled={readOnly}
            value={current?.ref ?? ""}
            onChange={(e) => {
              const picked = byRef.get(e.target.value) ?? null;
              if (field.many) {
                if (picked) onChange([...selected, picked]);
              } else onChange(picked);
            }}
          >
            <option value="">{field.many ? `Add ${target}…` : "— none —"}</option>
            {current && !byRef.has(current.ref) && (
              <option value={current.ref}>{name(current)} (not in the 200 most recent)</option>
            )}
            {(field.many ? available : options).map((o) => (
              <option key={o.ref} value={o.ref}>
                {o.label || o.ref}
              </option>
            ))}
          </select>
        )}
        {target === "Asset" && !readOnly && (
          <>
            <input
              ref={fileInput}
              type="file"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0];
                e.target.value = "";
                if (file) void upload(file);
              }}
            />
            <button
              type="button"
              className="btn btn-sm"
              disabled={uploading}
              onClick={() => fileInput.current?.click()}
            >
              {uploading && <span className="loading loading-spinner loading-xs" aria-hidden="true" />}
              Upload…
            </button>
          </>
        )}
      </div>
      {current?.url && (
        <a href={current.url} target="_blank" rel="noreferrer" className="link text-xs self-start">
          Open file
        </a>
      )}
      {error && <p className="text-error text-sm">{error}</p>}
    </div>
  );
}

// ---- components -----------------------------------------------------------------

function Fields({
  componentName,
  value,
  onChange,
  models,
  readOnly,
}: {
  componentName: string;
  value: FormValues;
  onChange: (v: FormValues) => void;
  models: Models;
  readOnly: boolean;
}) {
  const c = models.components.get(componentName);
  if (!c) return <p className="text-error text-sm">Unknown component {componentName}.</p>;
  return (
    <div className="flex flex-col gap-1">
      {c.fields.map((f) => (
        <FieldInput
          key={f.name}
          field={f}
          value={value[f.name]}
          onChange={(v) => onChange({ ...value, [f.name]: v })}
          models={models}
          readOnly={readOnly}
          existing={false}
        />
      ))}
    </div>
  );
}

function ItemCard({
  title,
  index,
  count,
  readOnly,
  onMove,
  onRemove,
  children,
}: {
  title: string;
  index: number;
  count: number;
  readOnly: boolean;
  onMove: (to: number) => void;
  onRemove: () => void;
  children: ReactNode;
}) {
  return (
    <li className="bg-base-200 rounded-box p-3">
      <div className="mb-1 flex items-center justify-between gap-2">
        <span className="text-sm font-semibold">
          {index + 1}. {title}
        </span>
        {!readOnly && (
          <span className="join">
            <button
              type="button"
              className="btn btn-ghost btn-xs join-item"
              disabled={index === 0}
              aria-label={`Move item ${index + 1} up`}
              onClick={() => onMove(index - 1)}
            >
              ↑
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-xs join-item"
              disabled={index === count - 1}
              aria-label={`Move item ${index + 1} down`}
              onClick={() => onMove(index + 1)}
            >
              ↓
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-xs join-item text-error"
              aria-label={`Remove item ${index + 1}`}
              onClick={onRemove}
            >
              Remove
            </button>
          </span>
        )}
      </div>
      {children}
    </li>
  );
}

function move<T>(list: T[], from: number, to: number): T[] {
  const copy = [...list];
  const [item] = copy.splice(from, 1);
  copy.splice(to, 0, item);
  return copy;
}

interface NestedProps {
  field: ContentTypeField;
  value: unknown;
  onChange: (v: unknown) => void;
  models: Models;
  readOnly: boolean;
}

function ComponentInput({ field, value, onChange, models, readOnly }: NestedProps) {
  const name = field.component ?? "";
  if (!field.repeatable) {
    if (!value) {
      return readOnly ? (
        <p className="text-base-content/60 text-sm">Empty.</p>
      ) : (
        <button
          type="button"
          className="btn btn-sm self-start"
          onClick={() => onChange(newComponentValue(name, models))}
        >
          Add {name}
        </button>
      );
    }
    return (
      <div className="flex flex-col gap-2">
        <Fields
          componentName={name}
          value={value as FormValues}
          onChange={onChange}
          models={models}
          readOnly={readOnly}
        />
        {!readOnly && !field.required && (
          <button type="button" className="btn btn-ghost btn-xs text-error self-start" onClick={() => onChange(null)}>
            Remove {name}
          </button>
        )}
      </div>
    );
  }
  const items = (value as FormValues[] | null) ?? [];
  return (
    <div className="flex flex-col gap-2">
      {items.length > 0 && (
        <ol className="flex flex-col gap-2">
          {items.map((item, i) => (
            <ItemCard
              key={i}
              title={name}
              index={i}
              count={items.length}
              readOnly={readOnly}
              onMove={(to) => onChange(move(items, i, to))}
              onRemove={() => onChange(items.filter((_, j) => j !== i))}
            >
              <Fields
                componentName={name}
                value={item}
                onChange={(v) => onChange(items.map((x, j) => (j === i ? v : x)))}
                models={models}
                readOnly={readOnly}
              />
            </ItemCard>
          ))}
        </ol>
      )}
      {!readOnly && (
        <button
          type="button"
          className="btn btn-sm self-start"
          onClick={() => onChange([...items, newComponentValue(name, models)])}
        >
          Add {name}
        </button>
      )}
      {readOnly && items.length === 0 && <p className="text-base-content/60 text-sm">Empty.</p>}
    </div>
  );
}

function ZoneInput({ field, value, onChange, models, readOnly }: NestedProps) {
  const items = (value as FormValues[] | null) ?? [];
  const allowed = field.components ?? [];
  const [pick, setPick] = useState(allowed[0] ?? "");
  return (
    <div className="flex flex-col gap-2">
      {items.length > 0 && (
        <ol className="flex flex-col gap-2">
          {items.map((item, i) => {
            const name = String(item.__component);
            return (
              <ItemCard
                key={i}
                title={name}
                index={i}
                count={items.length}
                readOnly={readOnly}
                onMove={(to) => onChange(move(items, i, to))}
                onRemove={() => onChange(items.filter((_, j) => j !== i))}
              >
                <Fields
                  componentName={name}
                  value={item}
                  onChange={(v) => onChange(items.map((x, j) => (j === i ? { ...v, __component: name } : x)))}
                  models={models}
                  readOnly={readOnly}
                />
              </ItemCard>
            );
          })}
        </ol>
      )}
      {!readOnly && allowed.length > 0 && (
        <div className="join self-start">
          <select
            className="select select-sm join-item"
            aria-label={`Block to add to ${field.name}`}
            value={pick}
            onChange={(e) => setPick(e.target.value)}
          >
            {allowed.map((c) => (
              <option key={c}>{c}</option>
            ))}
          </select>
          <button
            type="button"
            className="btn btn-sm join-item"
            onClick={() => onChange([...items, { __component: pick, ...newComponentValue(pick, models) }])}
          >
            Add block
          </button>
        </div>
      )}
      {readOnly && items.length === 0 && <p className="text-base-content/60 text-sm">Empty.</p>}
    </div>
  );
}
