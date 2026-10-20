// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { gql } from "./graphql";

export type FormFieldType =
  | "TEXT"
  | "TEXTAREA"
  | "EMAIL"
  | "NUMBER"
  | "DATE"
  | "DATETIME"
  | "SELECT"
  | "MULTI_SELECT"
  | "CHECKBOX"
  | "URL"
  | "PHONE";

export const FIELD_TYPES: { value: FormFieldType; label: string }[] = [
  { value: "TEXT", label: "Text" },
  { value: "TEXTAREA", label: "Long text" },
  { value: "EMAIL", label: "Email" },
  { value: "PHONE", label: "Phone" },
  { value: "NUMBER", label: "Number" },
  { value: "DATE", label: "Date" },
  { value: "DATETIME", label: "Date and time" },
  { value: "SELECT", label: "Choice" },
  { value: "MULTI_SELECT", label: "Multiple choice" },
  { value: "CHECKBOX", label: "Checkbox" },
  { value: "URL", label: "URL" },
];

export interface FormField {
  name: string;
  label: string;
  type: FormFieldType;
  required: boolean;
  help: string | null;
  placeholder: string | null;
  options: string[] | null;
  maxLength: number | null;
  min: number | null;
  max: number | null;
}

export interface Form {
  id: string;
  slug: string;
  title: string;
  description: string | null;
  fields: FormField[];
  enabled: boolean;
  successMessage: string | null;
  redirectUrl: string | null;
  submissionCount: number;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

export interface FormSubmission {
  id: string;
  formId: string;
  data: Record<string, unknown>;
  submittedAt: string;
  source: string | null;
}

export type FormInput = Pick<Form, "slug" | "title" | "description" | "enabled" | "successMessage" | "redirectUrl"> & {
  fields: FormField[];
};

const FIELD = "name label type required help placeholder options maxLength min max";
const FORM = `id slug title description enabled successMessage redirectUrl submissionCount createdBy createdAt updatedAt fields { ${FIELD} }`;

export async function fetchForms() {
  const data = await gql<{ forms: Form[] }>(`query Forms { forms { ${FORM} } }`);
  return data.forms;
}

/** Drops null optional values so the input matches FormFieldInput. */
function fieldInput(f: FormField) {
  return Object.fromEntries(
    Object.entries(f).filter(
      ([k, v]) => v !== null && v !== "" && !(k === "options" && Array.isArray(v) && v.length === 0),
    ),
  );
}

export async function saveForm(id: string | null, input: FormInput) {
  const formInput = { ...input, fields: input.fields.map(fieldInput) };
  if (id) {
    const data = await gql<{ updateForm: Form }, { id: string; input: typeof formInput }>(
      `mutation UpdateForm($id: ID!, $input: FormInput!) { updateForm(id: $id, input: $input) { ${FORM} } }`,
      { id, input: formInput },
    );
    return data.updateForm;
  }
  const data = await gql<{ createForm: Form }, { input: typeof formInput }>(
    `mutation CreateForm($input: FormInput!) { createForm(input: $input) { ${FORM} } }`,
    { input: formInput },
  );
  return data.createForm;
}

export async function deleteForm(id: string) {
  await gql<{ deleteForm: string }, { id: string }>(`mutation DeleteForm($id: ID!) { deleteForm(id: $id) }`, { id });
}

export async function fetchSubmissions(formId: string, first: number, skip: number) {
  const data = await gql<
    { formSubmissions: { total: number; items: FormSubmission[] } },
    { formId: string; first: number; skip: number }
  >(
    `query FormSubmissions($formId: ID!, $first: Int, $skip: Int) {
      formSubmissions(formId: $formId, first: $first, skip: $skip) { total items { id formId data submittedAt source } }
    }`,
    { formId, first, skip },
  );
  return data.formSubmissions;
}

export async function deleteSubmissions(formId: string, ids: string[]) {
  const data = await gql<{ deleteFormSubmissions: number }, { formId: string; ids: string[] }>(
    `mutation DeleteFormSubmissions($formId: ID!, $ids: [ID!]!) { deleteFormSubmissions(formId: $formId, ids: $ids) }`,
    { formId, ids },
  );
  return data.deleteFormSubmissions;
}

/** A submitted value as text (objects as JSON, never `[object Object]`). */
export function valueText(v: unknown): string {
  if (typeof v === "string") return v;
  if (typeof v === "number" || typeof v === "boolean") return `${v}`;
  return JSON.stringify(v) ?? "";
}

/** Submissions as CSV (one column per field, then the date and the source page). */
export function submissionsCsv(form: Form, items: FormSubmission[]): string {
  const cell = (v: unknown) => {
    const s = Array.isArray(v) ? v.join(", ") : v === undefined || v === null ? "" : valueText(v);
    return /[",\n;]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  const header = [...form.fields.map((f) => f.label), "Submitted at", "Page"];
  const rows = items.map((s) => [...form.fields.map((f) => s.data[f.name]), s.submittedAt, s.source]);
  return [header, ...rows].map((r) => r.map(cell).join(",")).join("\n") + "\n";
}
