// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { gql } from "./graphql";

export type FieldKind =
  | "TEXT"
  | "MARKDOWN"
  | "INT"
  | "FLOAT"
  | "BOOLEAN"
  | "DATETIME"
  | "DATE"
  | "JSON"
  | "UUID"
  | "PASSWORD"
  | "ENUM"
  | "RELATION"
  | "COMPONENT"
  | "DYNAMIC_ZONE";

export interface ContentTypeField {
  name: string;
  type: FieldKind;
  required: boolean;
  description: string | null;
  enumValues: string[] | null;
  relationTarget: string | null;
  many: boolean;
  /** TEXT and PASSWORD. */
  minLength: number | null;
  /** TEXT: null means unlimited. */
  maxLength: number | null;
  /** TEXT: regex the whole value must match. */
  pattern: string | null;
  component: string | null;
  repeatable: boolean;
  /** DYNAMIC_ZONE: allowed components. */
  components: string[] | null;
}

export type ContentTypeFieldInput = ContentTypeField;

export interface ContentType {
  apiId: string;
  pluralApiId: string;
  description: string | null;
  workflow: boolean;
  fields: ContentTypeField[];
}

export interface ContentTypeInput {
  apiId: string;
  pluralApiId: string;
  description: string | null;
  workflow: boolean;
  fields: ContentTypeFieldInput[];
}

export interface ComponentDefinition {
  apiId: string;
  description: string | null;
  fields: ContentTypeField[];
}

export interface ComponentInput {
  apiId: string;
  description: string | null;
  fields: ContentTypeFieldInput[];
}

const FIELD_SELECTION =
  "fields { name type required description enumValues relationTarget many minLength maxLength pattern component repeatable components }";
const CONTENT_TYPE_FIELDS = `apiId pluralApiId description workflow ${FIELD_SELECTION}`;
const COMPONENT_FIELDS = `apiId description ${FIELD_SELECTION}`;

export function fetchContentTypes() {
  return gql<{ contentTypes: ContentType[] }>(`query ContentModels { contentTypes { ${CONTENT_TYPE_FIELDS} } }`);
}

export function fetchComponents() {
  return gql<{ components: ComponentDefinition[] }>(`query Components { components { ${COMPONENT_FIELDS} } }`);
}

/** Both lists, for editors (relation targets, components to use). */
export async function fetchModels() {
  const [{ contentTypes }, { components }] = await Promise.all([fetchContentTypes(), fetchComponents()]);
  return { contentTypes, components };
}

function fieldInput(f: ContentTypeField): ContentTypeFieldInput {
  return {
    name: f.name,
    type: f.type,
    required: f.required,
    description: f.description,
    enumValues: f.enumValues,
    relationTarget: f.relationTarget,
    many: f.many,
    minLength: f.minLength,
    // Sent explicitly: null keeps "unlimited" (an absent key would mean 80).
    maxLength: f.maxLength,
    pattern: f.pattern,
    component: f.component,
    repeatable: f.repeatable,
    components: f.components,
  };
}

/** `updateContentType` REPLACES the definition: rebuild the complete input from the current one. */
export function toInput(type: ContentType): ContentTypeInput {
  return {
    apiId: type.apiId,
    pluralApiId: type.pluralApiId,
    description: type.description,
    workflow: type.workflow,
    fields: type.fields.map(fieldInput),
  };
}

export function toComponentInput(c: ComponentDefinition): ComponentInput {
  return { apiId: c.apiId, description: c.description, fields: c.fields.map(fieldInput) };
}

export async function updateContentType(apiId: string, input: ContentTypeInput): Promise<ContentType> {
  const data = await gql<{ updateContentType: ContentType }, { apiId: string; input: ContentTypeInput }>(
    `mutation UpdateContentType($apiId: String!, $input: ContentTypeInput!) {
      updateContentType(apiId: $apiId, input: $input) { ${CONTENT_TYPE_FIELDS} }
    }`,
    { apiId, input },
  );
  return data.updateContentType;
}

/**
 * Flip the review workflow of a content type. The definition is re-fetched right
 * before the update so a concurrent edit of the fields is not overwritten.
 */
export async function setWorkflow(apiId: string, workflow: boolean): Promise<ContentType> {
  const { contentTypes } = await fetchContentTypes();
  const current = contentTypes.find((t) => t.apiId === apiId);
  if (!current) throw new Error(`Content type ${apiId} no longer exists.`);
  return updateContentType(apiId, { ...toInput(current), workflow });
}

/** Content types managed by Tess itself (media library); the API refuses to change them. */
export const BUILTIN_TYPES: readonly string[] = ["Asset"];

export const FIELD_KINDS: { kind: FieldKind; label: string; hint: string; inComponents: boolean }[] = [
  {
    kind: "TEXT",
    label: "Text",
    hint: "Titles, names, emails… Length bounds (default max 80) and an optional format.",
    inComponents: true,
  },
  {
    kind: "MARKDOWN",
    label: "Markdown",
    hint: "Rich text as Markdown/MDX, rendered by your clients. HTML tags are refused.",
    inComponents: true,
  },
  { kind: "INT", label: "Integer", hint: "Whole numbers", inComponents: true },
  { kind: "FLOAT", label: "Number", hint: "Decimals", inComponents: true },
  { kind: "BOOLEAN", label: "Boolean", hint: "Yes / no", inComponents: true },
  { kind: "DATETIME", label: "Date & time", hint: "RFC 3339, stored in UTC", inComponents: true },
  { kind: "DATE", label: "Date", hint: "Calendar date without time (YYYY-MM-DD)", inComponents: true },
  { kind: "JSON", label: "JSON", hint: "Any structured value", inComponents: true },
  { kind: "UUID", label: "UUID", hint: "Generated automatically when left empty", inComponents: true },
  {
    kind: "PASSWORD",
    label: "Password",
    hint: "Stored hashed, never returned by the API (check it with verifyPassword)",
    inComponents: false,
  },
  { kind: "ENUM", label: "Enumeration", hint: "One value from a fixed list", inComponents: true },
  {
    kind: "RELATION",
    label: "Relation",
    hint: "Link to entries of a content type (use Asset for media)",
    inComponents: true,
  },
  {
    kind: "COMPONENT",
    label: "Component",
    hint: "A reusable group of fields, single or repeatable",
    inComponents: true,
  },
  {
    kind: "DYNAMIC_ZONE",
    label: "Dynamic zone",
    hint: "An ordered list mixing several components (page builder)",
    inComponents: false,
  },
];

/** Common formats for TEXT fields (Rust `regex` syntax; the whole value must match). */
export const TEXT_FORMATS: { label: string; pattern: string }[] = [
  { label: "Email", pattern: "[^@\\s]+@[^@\\s]+\\.[A-Za-z]{2,}" },
  { label: "URL (http/https)", pattern: "https?://\\S+" },
  { label: "Slug", pattern: "[a-z0-9]+(?:-[a-z0-9]+)*" },
  { label: "Phone (international)", pattern: "\\+[1-9][0-9 ]{6,18}" },
];

export async function createContentType(input: ContentTypeInput): Promise<ContentType> {
  const data = await gql<{ createContentType: ContentType }, { input: ContentTypeInput }>(
    `mutation CreateContentType($input: ContentTypeInput!) {
      createContentType(input: $input) { ${CONTENT_TYPE_FIELDS} }
    }`,
    { input },
  );
  return data.createContentType;
}

export async function deleteContentType(apiId: string): Promise<void> {
  await gql<{ deleteContentType: string }, { apiId: string }>(
    `mutation DeleteContentType($apiId: String!) { deleteContentType(apiId: $apiId) }`,
    { apiId },
  );
}

export async function createComponent(input: ComponentInput): Promise<ComponentDefinition> {
  const data = await gql<{ createComponent: ComponentDefinition }, { input: ComponentInput }>(
    `mutation CreateComponent($input: ComponentDefinitionInput!) { createComponent(input: $input) { ${COMPONENT_FIELDS} } }`,
    { input },
  );
  return data.createComponent;
}

export async function updateComponent(apiId: string, input: ComponentInput): Promise<ComponentDefinition> {
  const data = await gql<{ updateComponent: ComponentDefinition }, { apiId: string; input: ComponentInput }>(
    `mutation UpdateComponent($apiId: String!, $input: ComponentDefinitionInput!) {
      updateComponent(apiId: $apiId, input: $input) { ${COMPONENT_FIELDS} }
    }`,
    { apiId, input },
  );
  return data.updateComponent;
}

export async function deleteComponent(apiId: string): Promise<void> {
  await gql<{ deleteComponent: string }, { apiId: string }>(
    `mutation DeleteComponent($apiId: String!) { deleteComponent(apiId: $apiId) }`,
    { apiId },
  );
}

/** Content types and components whose fields use the component `apiId`. */
export function componentUsers(apiId: string, types: ContentType[], components: ComponentDefinition[]): string[] {
  const uses = (fields: ContentTypeField[]) =>
    fields.some((f) => f.component === apiId || (f.components ?? []).includes(apiId));
  return [
    ...types.filter((t) => uses(t.fields)).map((t) => t.apiId),
    ...components.filter((c) => c.apiId !== apiId && uses(c.fields)).map((c) => c.apiId),
  ];
}

// Client-side checks mirroring the server rules (crates/core/src/validate.rs),
// for immediate feedback; the server remains the authority (e.g. for regex syntax).
const TYPE_NAME = /^[A-Z][A-Za-z0-9]{0,63}$/;
const FIELD_NAME = /^[a-z][A-Za-z0-9]{0,63}$/;
const ENUM_VALUE = /^[A-Za-z_][A-Za-z0-9_]{0,63}$/;
const RESERVED_FIELDS = new Set([
  "id",
  "createdAt",
  "updatedAt",
  "publishedAt",
  "workflowStatus",
  "createdBy",
  "updatedBy",
  "history",
  "ref",
  "stage",
  "documentInStages",
  "where",
  "orderBy",
]);

export function defaultPlural(apiId: string): string {
  return apiId ? `${apiId[0].toLowerCase()}${apiId.slice(1)}s` : "";
}

export interface ValidationErrors {
  apiId?: string;
  pluralApiId?: string;
  form?: string;
  /** Per field index. */
  fields: Record<number, string>;
}

export type ModelKind = "contentType" | "component";

export function validateFields(fields: ContentTypeFieldInput[], kind: ModelKind): Record<number, string> {
  const errors: Record<number, string> = {};
  const seen = new Set<string>();
  fields.forEach((f, i) => {
    let message: string | undefined;
    if (!FIELD_NAME.test(f.name)) message = "Name: camelCase letters and digits (e.g. coverImage).";
    else if (RESERVED_FIELDS.has(f.name)) message = `"${f.name}" is reserved.`;
    else if (seen.has(f.name)) message = `Duplicate field "${f.name}".`;
    else if (kind === "component" && !FIELD_KINDS.find((k) => k.kind === f.type)?.inComponents)
      message = "This type is not allowed inside components.";
    else if (f.type === "TEXT" && f.minLength != null && f.maxLength != null && f.minLength > f.maxLength)
      message = "The minimum length is greater than the maximum.";
    else if (f.type === "TEXT" && f.maxLength === 0) message = "The maximum length must be at least 1.";
    else if (f.type === "PASSWORD" && (f.minLength ?? 8) < 1) message = "The minimum length must be at least 1.";
    else if (f.type === "ENUM") {
      const values = f.enumValues ?? [];
      if (values.length === 0) message = "Add at least one enumeration value.";
      else if (values.some((v) => !ENUM_VALUE.test(v))) message = "Enum values: letters, digits and _ (e.g. IN_STOCK).";
      else if (new Set(values).size !== values.length) message = "Duplicate enumeration values.";
    } else if (f.type === "RELATION" && !f.relationTarget) message = "Choose the related content type.";
    else if (f.type === "COMPONENT" && !f.component) message = "Choose the component.";
    else if (f.type === "DYNAMIC_ZONE" && !(f.components ?? []).length)
      message = "Choose at least one component for the zone.";
    seen.add(f.name);
    if (message) errors[i] = message;
  });
  return errors;
}

export function validateInput(
  input: ContentTypeInput,
  existing: ContentType[],
  editing: string | null,
  components: ComponentDefinition[] = [],
): ValidationErrors {
  const errors: ValidationErrors = { fields: validateFields(input.fields, "contentType") };
  if (!TYPE_NAME.test(input.apiId)) {
    errors.apiId = "PascalCase letters and digits, starting with an uppercase letter (e.g. BlogPost).";
  } else if (!editing && existing.some((t) => t.apiId === input.apiId)) {
    errors.apiId = "A content type with this API ID already exists.";
  } else if (!editing && components.some((c) => c.apiId === input.apiId)) {
    errors.apiId = "A component already uses this API ID.";
  }
  const plural = input.pluralApiId || defaultPlural(input.apiId);
  if (!FIELD_NAME.test(plural)) {
    errors.pluralApiId = "camelCase letters and digits, starting with a lowercase letter (e.g. blogPosts).";
  } else if (plural === defaultPlural(input.apiId).slice(0, -1)) {
    errors.pluralApiId = "Must differ from the singular form.";
  }
  if (input.fields.length === 0) errors.form = "Add at least one field.";
  return errors;
}

export function validateComponentInput(
  input: ComponentInput,
  types: ContentType[],
  existing: ComponentDefinition[],
  editing: string | null,
): ValidationErrors {
  const errors: ValidationErrors = { fields: validateFields(input.fields, "component") };
  if (!TYPE_NAME.test(input.apiId)) {
    errors.apiId = "PascalCase letters and digits, starting with an uppercase letter (e.g. SeoMeta).";
  } else if (!editing && existing.some((c) => c.apiId === input.apiId)) {
    errors.apiId = "A component with this API ID already exists.";
  } else if (types.some((t) => t.apiId === input.apiId)) {
    errors.apiId = "A content type already uses this API ID.";
  }
  if (input.fields.length === 0) errors.form = "Add at least one field.";
  return errors;
}

export function hasErrors(e: ValidationErrors): boolean {
  return Boolean(e.apiId || e.pluralApiId || e.form || Object.keys(e.fields).length);
}
