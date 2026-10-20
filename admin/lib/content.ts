// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

/**
 * Content manager data layer: builds GraphQL operations from the model
 * definitions, and converts between API values and form values.
 */

import type { ComponentDefinition, ContentType, ContentTypeField } from "./contentTypes";
import { ApiError, authHeader, gql, handleAuthFailure } from "./graphql";

export interface Models {
  types: Map<string, ContentType>;
  components: Map<string, ComponentDefinition>;
}

export type WorkflowStatus = "DRAFT" | "IN_REVIEW" | "CHANGES_REQUESTED" | "PUBLISHED";

export interface EntrySummary {
  id: string;
  ref: string;
  workflowStatus: WorkflowStatus;
  publishedAt: string | null;
  updatedAt: string;
  updatedBy: string | null;
  label: string;
}

/**
 * A form value: same shape as the API output, with relations as
 * `RelationOption`s (ids change on every read; `ref` is stable).
 */
export type FormValues = Record<string, unknown>;

/** Scalar API value as text ("" for null; objects as JSON). */
function text(v: unknown): string {
  if (v == null) return "";
  if (typeof v === "string") return v;
  if (typeof v === "number" || typeof v === "boolean") return String(v);
  return JSON.stringify(v);
}

const lowerFirst = (s: string) => (s ? s[0].toLowerCase() + s.slice(1) : s);

/** Field used to name entries in lists and pickers (first TEXT field). */
export function labelField(fields: ContentTypeField[]): string | null {
  return fields.find((f) => f.type === "TEXT")?.name ?? fields.find((f) => f.type === "MARKDOWN")?.name ?? null;
}

function targetLabel(target: string, models: Models): string {
  if (target === "Asset") return "fileName";
  const t = models.types.get(target);
  return (t && labelField(t.fields)) ?? "";
}

/** GraphQL selection set for fields (recursive through components). */
export function selection(fields: ContentTypeField[], models: Models, depth = 0): string {
  if (depth > 6) return "";
  return fields
    .map((f) => {
      switch (f.type) {
        case "PASSWORD":
          return ""; // write-only
        case "RELATION":
          return `${f.name} { id ref ${targetLabel(f.relationTarget ?? "", models)}${f.relationTarget === "Asset" ? " url" : ""} }`;
        case "COMPONENT": {
          const c = models.components.get(f.component ?? "");
          return c ? `${f.name} { ${selection(c.fields, models, depth + 1) || "__typename"} }` : "";
        }
        case "DYNAMIC_ZONE":
          return `${f.name} { __typename ${(f.components ?? [])
            .map((name) => {
              const c = models.components.get(name);
              return c ? `... on ${name} { ${selection(c.fields, models, depth + 1) || "__typename"} }` : "";
            })
            .join(" ")} }`;
        default:
          return f.name;
      }
    })
    .filter(Boolean)
    .join(" ");
}

export async function listEntries(
  type: ContentType,
  options: { first: number; skip: number; search: string },
): Promise<{ total: number; items: EntrySummary[] }> {
  const label = labelField(type.fields);
  const where = label && options.search.trim() ? { [`${label}_contains`]: options.search.trim() } : undefined;
  const data = await gql<Record<string, { total: number; items: Record<string, unknown>[] }>, Record<string, unknown>>(
    `query ContentList($first: Int, $skip: Int, $where: ${type.apiId}WhereInput) {
      list: ${type.pluralApiId}(stage: DRAFT, first: $first, skip: $skip, where: $where, orderBy: [updatedAt_DESC]) {
        total items { id ref workflowStatus publishedAt updatedAt updatedBy ${label ?? ""} }
      }
    }`,
    { first: options.first, skip: options.skip, where },
  );
  return {
    total: data.list.total,
    items: data.list.items.map((i) => ({
      ...(i as unknown as EntrySummary),
      label: label ? text(i[label]) : "",
    })),
  };
}

export async function loadEntry(
  type: ContentType,
  id: string,
  models: Models,
): Promise<{ id: string; workflowStatus: WorkflowStatus; publishedAt: string | null; values: FormValues }> {
  const data = await gql<{ entry: Record<string, unknown> | null }, { id: string }>(
    `query ContentEntry($id: ID!) {
      entry: ${lowerFirst(type.apiId)}(id: $id, stage: DRAFT) { id workflowStatus publishedAt ${selection(type.fields, models)} }
    }`,
    { id },
  );
  if (!data.entry) throw new ApiError("This entry no longer exists.", "NOT_FOUND");
  return {
    id: String(data.entry.id),
    workflowStatus: data.entry.workflowStatus as WorkflowStatus,
    publishedAt: (data.entry.publishedAt as string | null) ?? null,
    values: toFormValues(type.fields, data.entry, models),
  };
}

export interface RelationOption {
  id: string;
  ref: string;
  label: string;
  url?: string;
}

const optionCache = new Map<string, Promise<RelationOption[]>>();

/** Ids expire: forget the options when an editor opens. */
export function resetRelationOptions() {
  optionCache.clear();
}

/** Entries offered by relation pickers (drafts included, newest first, up to 200). */
export function relationOptions(target: string, models: Models): Promise<RelationOption[]> {
  const cached = optionCache.get(target);
  if (cached) return cached;
  const type = models.types.get(target);
  const plural = target === "Asset" ? "assets" : type?.pluralApiId;
  const label = targetLabel(target, models);
  const extra = target === "Asset" ? "url" : "";
  const promise = plural
    ? gql<{ list: { items: Record<string, unknown>[] } }>(
        `query RelationOptions { list: ${plural}(stage: DRAFT, first: 200, orderBy: [updatedAt_DESC]) { items { id ref ${label} ${extra} } } }`,
      ).then((d) =>
        d.list.items.map((i) => ({
          id: String(i.id),
          ref: String(i.ref),
          label: label ? text(i[label]) : text(i.ref),
          url: i.url ? text(i.url) : undefined,
        })),
      )
    : Promise.resolve([]);
  optionCache.set(target, promise);
  promise.catch(() => optionCache.delete(target));
  return promise;
}

/** Uploads a file to the media library (Contributor and above) and returns it as an option. */
export async function uploadAsset(file: File): Promise<RelationOption> {
  const res = await fetch(`/media/upload?fileName=${encodeURIComponent(file.name)}`, {
    method: "POST",
    headers: { "Content-Type": file.type || "application/octet-stream", ...authHeader() },
    body: file,
  });
  const body = (await res.json().catch(() => null)) as Record<string, string> | null;
  if (!res.ok) {
    handleAuthFailure(res.status, body?.code, false);
    throw new ApiError(body?.error ?? `Upload failed (HTTP ${res.status}).`, body?.code, res.status);
  }
  optionCache.delete("Asset");
  return { id: body!.id, ref: body!.ref, label: body!.fileName, url: body!.url };
}

// ---- value conversion -------------------------------------------------------

/** `2026-10-18T06:22:55.274Z` → `2026-10-18T08:22` (local, for datetime-local inputs). */
export function isoToLocal(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function fieldDefaults(fields: ContentTypeField[]): FormValues {
  return Object.fromEntries(fields.map((f) => [f.name, emptyValue(f)]));
}

export function emptyValue(f: ContentTypeField): unknown {
  switch (f.type) {
    case "BOOLEAN":
      return false;
    case "RELATION":
      return f.many ? [] : null;
    case "COMPONENT":
      return f.repeatable ? [] : null;
    case "DYNAMIC_ZONE":
      return [];
    default:
      return "";
  }
}

export function newComponentValue(name: string, models: Models): FormValues {
  const c = models.components.get(name);
  return c ? fieldDefaults(c.fields) : {};
}

export function emptyForm(type: ContentType): FormValues {
  return fieldDefaults(type.fields);
}

/** API output → form values. */
export function toFormValues(fields: ContentTypeField[], data: Record<string, unknown>, models: Models): FormValues {
  const out: FormValues = {};
  for (const f of fields) {
    const v = data[f.name];
    switch (f.type) {
      case "PASSWORD":
        out[f.name] = ""; // never returned; empty means "unchanged"
        break;
      case "RELATION": {
        const label = targetLabel(f.relationTarget ?? "", models);
        const one = (x: Record<string, unknown>): RelationOption => ({
          id: String(x.id),
          ref: String(x.ref),
          label: label ? text(x[label]) : "",
          url: x.url ? text(x.url) : undefined,
        });
        out[f.name] = f.many
          ? ((v as Record<string, unknown>[] | null) ?? []).map(one)
          : v
            ? one(v as Record<string, unknown>)
            : null;
        break;
      }
      case "COMPONENT": {
        const c = models.components.get(f.component ?? "");
        const one = (x: unknown) => (c && x ? toFormValues(c.fields, x as Record<string, unknown>, models) : null);
        out[f.name] = f.repeatable ? ((v as unknown[] | null) ?? []).map(one).filter(Boolean) : one(v);
        break;
      }
      case "DYNAMIC_ZONE":
        out[f.name] = ((v as Record<string, unknown>[] | null) ?? []).flatMap((item) => {
          const name = String(item.__typename);
          const c = models.components.get(name);
          return c ? [{ __component: name, ...toFormValues(c.fields, item, models) }] : [];
        });
        break;
      case "DATETIME":
        out[f.name] = typeof v === "string" ? isoToLocal(v) : "";
        break;
      case "JSON":
        out[f.name] = v == null ? "" : JSON.stringify(v, null, 2);
        break;
      case "INT":
      case "FLOAT":
        out[f.name] = text(v);
        break;
      case "BOOLEAN":
        out[f.name] = v === true;
        break;
      default:
        out[f.name] = text(v);
    }
  }
  return out;
}

export class FormError extends Error {}

/**
 * Form values → GraphQL input. Empty optional values become `null` (which
 * clears them); empty passwords are omitted (kept unchanged).
 */
export function toInput(
  fields: ContentTypeField[],
  values: FormValues,
  models: Models,
  path = "",
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const f of fields) {
    const v = values[f.name];
    const at = `${path}${f.name}`;
    const blank = v === "" || v == null;
    switch (f.type) {
      case "PASSWORD":
        if (!blank) out[f.name] = v;
        break;
      case "INT":
      case "FLOAT": {
        if (blank) {
          out[f.name] = null;
          break;
        }
        const n = Number(v);
        if (!Number.isFinite(n) || (f.type === "INT" && !Number.isInteger(n))) {
          throw new FormError(`${at}: expected ${f.type === "INT" ? "an integer" : "a number"}.`);
        }
        out[f.name] = n;
        break;
      }
      case "BOOLEAN":
        out[f.name] = v === true;
        break;
      case "DATETIME":
        out[f.name] = blank ? null : new Date(text(v)).toISOString();
        break;
      case "JSON":
        if (blank) out[f.name] = null;
        else {
          try {
            out[f.name] = JSON.parse(text(v));
          } catch {
            throw new FormError(`${at}: invalid JSON.`);
          }
        }
        break;
      case "RELATION":
        out[f.name] = f.many
          ? ((v as RelationOption[] | null) ?? []).map((r) => r.id)
          : ((v as RelationOption | null)?.id ?? null);
        break;
      case "COMPONENT": {
        const c = models.components.get(f.component ?? "");
        if (!c) break;
        if (f.repeatable) {
          out[f.name] = (v as FormValues[]).map((item, i) => toInput(c.fields, item, models, `${at}[${i}].`));
        } else {
          out[f.name] = v ? toInput(c.fields, v as FormValues, models, `${at}.`) : null;
        }
        break;
      }
      case "DYNAMIC_ZONE":
        out[f.name] = (v as FormValues[]).map((item, i) => {
          const name = String(item.__component);
          const c = models.components.get(name);
          return { [name]: c ? toInput(c.fields, item, models, `${at}[${i}].`) : {} };
        });
        break;
      default:
        out[f.name] = blank ? null : v;
    }
  }
  return out;
}

// ---- mutations ----------------------------------------------------------------

export async function saveEntry(type: ContentType, id: string | null, data: Record<string, unknown>): Promise<string> {
  if (id) {
    const r = await gql<{ m: { id: string } }, Record<string, unknown>>(
      `mutation UpdateEntry($id: ID!, $data: ${type.apiId}UpdateInput!) { m: update${type.apiId}(id: $id, data: $data) { id } }`,
      { id, data },
    );
    return r.m.id;
  }
  const r = await gql<{ m: { id: string } }, Record<string, unknown>>(
    `mutation CreateEntry($data: ${type.apiId}CreateInput!) { m: create${type.apiId}(data: $data) { id } }`,
    { data },
  );
  return r.m.id;
}

export type EntryAction = "publish" | "unpublish" | "submit" | "delete";

export async function entryAction(type: ContentType, id: string, action: EntryAction): Promise<void> {
  const field = {
    publish: `publish${type.apiId}(id: $id) { id }`,
    unpublish: `unpublish${type.apiId}(id: $id) { id }`,
    submit: `submit${type.apiId}ForReview(id: $id) { id }`,
    delete: `delete${type.apiId}(id: $id)`,
  }[action];
  await gql<unknown, { id: string }>(`mutation EntryAction($id: ID!) { m: ${field} }`, { id });
}
