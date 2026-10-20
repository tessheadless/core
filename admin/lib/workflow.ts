// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { gql } from "./graphql";

/*
 * Review workflow discovery through GraphQL introspection.
 *
 * Uses introspection (open to every signed-in caller). A content type T has the
 * review workflow when the Mutation type has an `approve<T>` field (and its
 * `requestChanges<T>` sibling). Its list query is the Query field returning
 * `<T>Connection` that takes a `where` argument (the `search<T>` field also
 * returns a connection but takes `query` instead), and its single-entry query
 * is the Query field returning `T` that takes an `id` argument.
 */

export type WorkflowStatus = "DRAFT" | "IN_REVIEW" | "CHANGES_REQUESTED" | "PUBLISHED";

export type HistoryAction =
  "CREATED" | "UPDATED" | "SUBMITTED" | "CHANGES_REQUESTED" | "APPROVED" | "PUBLISHED" | "UNPUBLISHED";

export interface HistoryEvent {
  action: HistoryAction;
  at: string;
  by: string;
  comment: string | null;
}

/** A content field of a workflow type that can be selected without sub-selection. */
export interface WorkflowField {
  name: string;
  /** GraphQL scalar or enum name (String, Int, DateTime, JSON, PageKindEnum…). */
  typeName: string;
  kind: "SCALAR" | "ENUM";
  list: boolean;
}

export interface WorkflowType {
  /** GraphQL object type name = content type apiId, e.g. "Page". */
  typeName: string;
  /** List query, e.g. "pages". */
  listField: string;
  /** Single-entry query, e.g. "page". */
  getField: string;
  approveMutation: string;
  requestChangesMutation: string;
  fields: WorkflowField[];
  /** First String field, used as the entry title in lists; null = fall back to `ref`. */
  titleField: string | null;
}

/** Fields every entry type has; they are queried explicitly, not as content. */
const SYSTEM_FIELDS = new Set([
  "id",
  "ref",
  "stage",
  "workflowStatus",
  "createdBy",
  "updatedBy",
  "createdAt",
  "updatedAt",
  "publishedAt",
  "history",
]);

interface TypeRef {
  kind: string;
  name: string | null;
  ofType: TypeRef | null;
}

interface IntrospectionField {
  name: string;
  args: { name: string }[];
  type: TypeRef;
}

interface IntrospectionType {
  kind: string;
  name: string;
  fields: IntrospectionField[] | null;
}

interface IntrospectionData {
  __schema: {
    queryType: { name: string };
    mutationType: { name: string } | null;
    types: IntrospectionType[];
  };
}

export const DISCOVERY_QUERY = `query WorkflowDiscovery {
  __schema {
    queryType { name }
    mutationType { name }
    types {
      kind
      name
      fields { name args { name } type { ...TypeRef } }
    }
  }
}
fragment TypeRef on __Type { kind name ofType { kind name ofType { kind name ofType { kind name } } } }`;

/** Strip NON_NULL / LIST wrappers. */
function unwrap(ref: TypeRef): { kind: string; name: string | null; list: boolean } {
  let list = false;
  let current: TypeRef | null = ref;
  while (current && (current.kind === "NON_NULL" || current.kind === "LIST")) {
    if (current.kind === "LIST") list = true;
    current = current.ofType;
  }
  return { kind: current?.kind ?? "", name: current?.name ?? null, list };
}

/** Pure part of the discovery, exported for testing against a captured introspection result. */
export function workflowTypesFromSchema(data: IntrospectionData): WorkflowType[] {
  const byName = new Map(data.__schema.types.map((t) => [t.name, t]));
  const queryFields = byName.get(data.__schema.queryType.name)?.fields ?? [];
  const mutationName = data.__schema.mutationType?.name;
  const mutationFields = new Set((mutationName ? byName.get(mutationName)?.fields : null)?.map((f) => f.name) ?? []);

  const result: WorkflowType[] = [];
  for (const mutation of mutationFields) {
    if (!mutation.startsWith("approve") || mutation.length <= "approve".length) continue;
    const typeName = mutation.slice("approve".length);
    const requestChangesMutation = `requestChanges${typeName}`;
    if (!mutationFields.has(requestChangesMutation)) continue;
    const object = byName.get(typeName);
    if (!object || object.kind !== "OBJECT" || !object.fields) continue;

    const list = queryFields.find((f) => {
      const t = unwrap(f.type);
      return !t.list && t.name === `${typeName}Connection` && f.args.some((a) => a.name === "where");
    });
    const get = queryFields.find((f) => {
      const t = unwrap(f.type);
      return !t.list && t.name === typeName && f.args.some((a) => a.name === "id");
    });
    if (!list || !get) continue;

    const fields: WorkflowField[] = [];
    for (const f of object.fields) {
      if (SYSTEM_FIELDS.has(f.name)) continue;
      const t = unwrap(f.type);
      // Relations are objects: they need a sub-selection and are skipped here.
      if ((t.kind === "SCALAR" || t.kind === "ENUM") && t.name)
        fields.push({ name: f.name, typeName: t.name, kind: t.kind, list: t.list });
    }
    const title = fields.find((f) => f.kind === "SCALAR" && f.typeName === "String" && !f.list);
    result.push({
      typeName,
      listField: list.name,
      getField: get.name,
      approveMutation: mutation,
      requestChangesMutation,
      fields,
      titleField: title?.name ?? null,
    });
  }
  return result.sort((a, b) => a.typeName.localeCompare(b.typeName));
}

export async function discoverWorkflowTypes(): Promise<WorkflowType[]> {
  return workflowTypesFromSchema(await gql<IntrospectionData>(DISCOVERY_QUERY));
}

// --- Queue -------------------------------------------------------------------

export interface QueueEntry {
  id: string;
  ref: string;
  title: string | null;
  createdBy: string | null;
  updatedBy: string | null;
  updatedAt: string;
  publishedAt: string | null;
}

export interface QueueList {
  total: number;
  items: QueueEntry[];
}

export const QUEUE_PAGE_SIZE = 50;

/** e.g. `query ReviewQueue_Page($status: WorkflowStatus!) { list: pages(where: {workflowStatus: $status}, stage: DRAFT, …) {…} }` */
export function queueQuery(type: WorkflowType): string {
  const title = type.titleField ? `title: ${type.titleField}` : "";
  return `query ReviewQueue_${type.typeName}($status: WorkflowStatus!, $first: Int) {
  list: ${type.listField}(where: { workflowStatus: $status }, stage: DRAFT, orderBy: [updatedAt_DESC], first: $first) {
    total
    items { id ref ${title} createdBy updatedBy updatedAt publishedAt }
  }
}`;
}

export async function fetchQueue(type: WorkflowType, status: WorkflowStatus): Promise<QueueList> {
  const data = await gql<{ list: QueueList }, { status: WorkflowStatus; first: number }>(queueQuery(type), {
    status,
    first: QUEUE_PAGE_SIZE,
  });
  return data.list;
}

// --- Detail ------------------------------------------------------------------

export type FieldValues = Record<string, unknown>;

export interface EntryDetail {
  id: string;
  ref: string;
  workflowStatus: WorkflowStatus;
  createdBy: string | null;
  updatedBy: string | null;
  createdAt: string;
  updatedAt: string;
  publishedAt: string | null;
  history: HistoryEvent[] | null;
  values: FieldValues;
}

function contentSelection(type: WorkflowType): string {
  return type.fields.map((f) => f.name).join(" ");
}

/** Draft + (when the entry was published before) the published version, for a side-by-side comparison. */
export function detailQuery(type: WorkflowType): string {
  const content = contentSelection(type);
  return `query ReviewEntry_${type.typeName}($id: ID!, $withPublished: Boolean!) {
  draft: ${type.getField}(id: $id, stage: DRAFT) {
    id ref workflowStatus createdBy updatedBy createdAt updatedAt publishedAt
    history { action at by comment }
    ${content}
  }
  published: ${type.getField}(id: $id, stage: PUBLISHED) @include(if: $withPublished) { ${content || "id"} }
}`;
}

type RawEntry = Omit<EntryDetail, "values"> & Record<string, unknown>;

function pickValues(type: WorkflowType, raw: Record<string, unknown> | null | undefined): FieldValues | null {
  if (!raw) return null;
  return Object.fromEntries(type.fields.map((f) => [f.name, raw[f.name]]));
}

export async function fetchEntryDetail(
  type: WorkflowType,
  id: string,
  withPublished: boolean,
): Promise<{ draft: EntryDetail | null; published: FieldValues | null }> {
  const data = await gql<
    { draft: RawEntry | null; published?: Record<string, unknown> | null },
    { id: string; withPublished: boolean }
  >(detailQuery(type), { id, withPublished });
  const raw = data.draft;
  const draft: EntryDetail | null = raw
    ? {
        id: raw.id,
        ref: raw.ref,
        workflowStatus: raw.workflowStatus,
        createdBy: raw.createdBy,
        updatedBy: raw.updatedBy,
        createdAt: raw.createdAt,
        updatedAt: raw.updatedAt,
        publishedAt: raw.publishedAt,
        history: raw.history,
        values: pickValues(type, raw) ?? {},
      }
    : null;
  return { draft, published: pickValues(type, data.published) };
}

// --- Actions -----------------------------------------------------------------

export async function approveEntry(type: WorkflowType, id: string, comment: string | null): Promise<void> {
  // Without a comment the argument is left out of the document (an explicit null is not accepted everywhere).
  const withComment = comment !== null;
  await gql<Record<string, unknown>, { id: string; comment?: string }>(
    `mutation Approve_${type.typeName}($id: ID!${withComment ? ", $comment: String!" : ""}) {
      ${type.approveMutation}(id: $id${withComment ? ", comment: $comment" : ""}) { id workflowStatus }
    }`,
    withComment ? { id, comment } : { id },
  );
}

export async function requestChanges(type: WorkflowType, id: string, comment: string): Promise<void> {
  await gql<Record<string, unknown>, { id: string; comment: string }>(
    `mutation RequestChanges_${type.typeName}($id: ID!, $comment: String!) { ${type.requestChangesMutation}(id: $id, comment: $comment) { id workflowStatus } }`,
    { id, comment },
  );
}

/** Display value of a field (JSON and lists stringified). */
export function displayValue(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value, null, 2);
}
