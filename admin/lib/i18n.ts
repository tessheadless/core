// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { ApiError, authHeader, gql, handleAuthFailure } from "./graphql";

export type TranslationState = "TRANSLATED" | "REVIEWED";
export type StateFilter = "UNTRANSLATED" | TranslationState;
export type TranslationFormat =
  | "JSON"
  | "JSON_NESTED"
  | "I18NEXT"
  | "ARB"
  | "XLIFF"
  | "PO"
  | "PROPERTIES"
  | "ANDROID_XML"
  | "APPLE_STRINGS"
  | "YAML"
  | "CSV";
export type StorageKind = "S3" | "AZURE_BLOB";
export type AzureAuth = "SHARED_KEY" | "SAS";

export interface Language {
  tag: string;
  name: string;
  base: boolean;
}

export interface Translation {
  language: string;
  text: string;
  state: TranslationState;
  updatedAt: string;
  updatedBy: string;
}

export interface TranslationKey {
  id: string;
  name: string;
  namespace: string;
  description: string | null;
  tags: string[];
  plural: boolean;
  translations: Translation[];
  createdAt: string;
  updatedAt: string;
}

export interface LanguageStats {
  language: string;
  total: number;
  translated: number;
  reviewed: number;
}

export interface FormatInfo {
  format: TranslationFormat;
  slug: string;
  contentType: string;
  importable: boolean;
  multiLanguage: boolean;
}

export interface TranslationSettings {
  public: boolean;
  servedStates: TranslationState[];
  fallbackToBase: boolean;
}

export interface DeliveryTarget {
  id: string;
  name: string;
  enabled: boolean;
  kind: StorageKind;
  provider: string;
  bucket: string;
  endpoint: string | null;
  region: string | null;
  forcePathStyle: boolean;
  accessKeyId: string | null;
  hasSecret: boolean;
  azureAuth: AzureAuth;
  pathPrefix: string;
  publicUrl: string | null;
  formats: TranslationFormat[];
  languages: string[];
  namespaces: string[];
  states: TranslationState[];
  fallbackToBase: boolean;
  autoPublish: boolean;
  lastPublishedAt: string | null;
  lastError: string | null;
  lastFileCount: number;
}

export interface PublishReport {
  targetId: string;
  name: string;
  ok: boolean;
  files: number;
  error: string | null;
  publishedAt: string;
}

export interface ImportReport {
  keysCreated: number;
  keysUpdated: number;
  translationsCreated: number;
  translationsUpdated: number;
  translationsSkipped: number;
  languages: string[];
}

export const FORMAT_LABELS: Record<TranslationFormat, string> = {
  JSON: "JSON (flat)",
  JSON_NESTED: "JSON (nested)",
  I18NEXT: "i18next JSON",
  ARB: "Flutter ARB",
  XLIFF: "XLIFF 1.2",
  PO: "Gettext PO",
  PROPERTIES: "Java .properties",
  ANDROID_XML: "Android strings.xml",
  APPLE_STRINGS: "Apple .strings",
  YAML: "YAML (Rails)",
  CSV: "CSV (all languages)",
};

const KEY_FIELDS = `id name namespace description tags plural createdAt updatedAt
  translations { language text state updatedAt updatedBy }`;
const TARGET_FIELDS = `id name enabled kind provider bucket endpoint region forcePathStyle accessKeyId hasSecret
  azureAuth pathPrefix publicUrl formats languages namespaces states fallbackToBase autoPublish
  lastPublishedAt lastError lastFileCount`;

export interface OverviewData {
  translationLanguages: Language[];
  translationStats: LanguageStats[];
  translationNamespaces: string[];
  translationTags: string[];
  translationFormats: FormatInfo[];
  translationSettings: TranslationSettings;
}

export function fetchOverview() {
  return gql<OverviewData>(`query TranslationsOverview {
    translationLanguages { tag name base }
    translationStats { language total translated reviewed }
    translationNamespaces
    translationTags
    translationFormats { format slug contentType importable multiLanguage }
    translationSettings { public servedStates fallbackToBase }
  }`);
}

export interface KeyFilter {
  search?: string;
  namespace?: string;
  tag?: string;
  language?: string;
  state?: StateFilter;
}

export async function fetchKeys(filter: KeyFilter, first: number, skip: number) {
  const data = await gql<
    { translationKeys: { items: TranslationKey[]; total: number } },
    { filter: KeyFilter; first: number; skip: number }
  >(
    `query TranslationKeys($filter: TranslationKeyFilter, $first: Int, $skip: Int) {
      translationKeys(filter: $filter, first: $first, skip: $skip) { total items { ${KEY_FIELDS} } }
    }`,
    { filter, first, skip },
  );
  return data.translationKeys;
}

export interface KeyInput {
  name: string;
  namespace: string;
  description?: string | null;
  tags?: string[];
  plural?: boolean;
  translations?: { language: string; text: string }[];
}

export async function createKey(input: KeyInput) {
  const data = await gql<{ createTranslationKey: TranslationKey }, { input: KeyInput }>(
    `mutation CreateTranslationKey($input: TranslationKeyInput!) { createTranslationKey(input: $input) { ${KEY_FIELDS} } }`,
    { input },
  );
  return data.createTranslationKey;
}

export async function updateKey(id: string, input: Partial<Omit<KeyInput, "translations">>) {
  const data = await gql<{ updateTranslationKey: TranslationKey }, { id: string; input: typeof input }>(
    `mutation UpdateTranslationKey($id: ID!, $input: TranslationKeyUpdateInput!) {
      updateTranslationKey(id: $id, input: $input) { ${KEY_FIELDS} }
    }`,
    { id, input },
  );
  return data.updateTranslationKey;
}

export async function deleteKeys(ids: string[]) {
  const data = await gql<{ deleteTranslationKeys: number }, { ids: string[] }>(
    `mutation DeleteTranslationKeys($ids: [ID!]!) { deleteTranslationKeys(ids: $ids) }`,
    { ids },
  );
  return data.deleteTranslationKeys;
}

/** `text: ""` removes the translation; omit `text` to only change the state. */
export async function setTranslation(keyId: string, language: string, text?: string, state?: TranslationState) {
  const data = await gql<
    { setTranslation: TranslationKey },
    { keyId: string; language: string; text?: string; state?: TranslationState }
  >(
    `mutation SetTranslation($keyId: ID!, $language: String!, $text: String, $state: TranslationState) {
      setTranslation(keyId: $keyId, language: $language, text: $text, state: $state) { ${KEY_FIELDS} }
    }`,
    { keyId, language, text, state },
  );
  return data.setTranslation;
}

export async function addLanguage(tag: string, name: string, base: boolean) {
  const data = await gql<{ addTranslationLanguage: Language }, { tag: string; name: string; base: boolean }>(
    `mutation AddLanguage($tag: String!, $name: String, $base: Boolean) {
      addTranslationLanguage(tag: $tag, name: $name, base: $base) { tag name base }
    }`,
    { tag, name, base },
  );
  return data.addTranslationLanguage;
}

export async function updateLanguage(tag: string, name?: string, base?: boolean) {
  const data = await gql<{ updateTranslationLanguage: Language }, { tag: string; name?: string; base?: boolean }>(
    `mutation UpdateLanguage($tag: String!, $name: String, $base: Boolean) {
      updateTranslationLanguage(tag: $tag, name: $name, base: $base) { tag name base }
    }`,
    { tag, name, base },
  );
  return data.updateTranslationLanguage;
}

export async function removeLanguage(tag: string) {
  await gql<{ removeTranslationLanguage: string }, { tag: string }>(
    `mutation RemoveLanguage($tag: String!) { removeTranslationLanguage(tag: $tag) }`,
    { tag },
  );
}

export interface ImportInput {
  format: TranslationFormat;
  content: string;
  language?: string | null;
  namespace: string;
  mode: "KEEP" | "OVERRIDE";
  state: TranslationState;
  tags: string[];
  createLanguages: boolean;
}

export async function importTranslations(input: ImportInput) {
  const data = await gql<{ importTranslations: ImportReport }, { input: ImportInput }>(
    `mutation ImportTranslations($input: TranslationImportInput!) {
      importTranslations(input: $input) {
        keysCreated keysUpdated translationsCreated translationsUpdated translationsSkipped languages
      }
    }`,
    { input },
  );
  return data.importTranslations;
}

export interface ExportParams {
  format: TranslationFormat;
  languages: string[];
  namespaces: string[];
  states: TranslationState[];
  fallbackToBase: boolean;
}

/** Downloads `/api/i18n/export` (one file, or a zip) and saves it through the browser. */
export async function downloadExport(params: ExportParams) {
  const query = new URLSearchParams({
    format: params.format,
    languages: params.languages.join(","),
    namespaces: params.namespaces.join(","),
    states: params.states.join(","),
    fallbackToBase: String(params.fallbackToBase),
  });
  let res: Response;
  try {
    res = await fetch(`/api/i18n/export?${query}`, { headers: authHeader() });
  } catch {
    throw new ApiError("Cannot reach the CMS server.", "NETWORK_ERROR");
  }
  if (!res.ok) {
    handleAuthFailure(res.status, undefined, false);
    const body = (await res.json().catch(() => null)) as { error?: string } | null;
    throw new ApiError(body?.error ?? `Export failed (HTTP ${res.status}).`, undefined, res.status);
  }
  const name = /filename="([^"]+)"/.exec(res.headers.get("Content-Disposition") ?? "")?.[1] ?? "translations";
  const url = URL.createObjectURL(await res.blob());
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  URL.revokeObjectURL(url);
}

export async function updateSettings(input: Partial<TranslationSettings>) {
  const data = await gql<{ updateTranslationSettings: TranslationSettings }, { input: typeof input }>(
    `mutation UpdateTranslationSettings($input: TranslationSettingsInput!) {
      updateTranslationSettings(input: $input) { public servedStates fallbackToBase }
    }`,
    { input },
  );
  return data.updateTranslationSettings;
}

export async function fetchTargets() {
  const data = await gql<{ translationDeliveryTargets: DeliveryTarget[] }>(
    `query DeliveryTargets { translationDeliveryTargets { ${TARGET_FIELDS} } }`,
  );
  return data.translationDeliveryTargets;
}

export type TargetInput = Partial<
  Omit<DeliveryTarget, "id" | "hasSecret" | "lastPublishedAt" | "lastError" | "lastFileCount">
> & {
  /** Write-only: omit to keep, null to remove. */
  secret?: string | null;
};

export async function saveTarget(id: string | null, input: TargetInput) {
  if (id) {
    const { kind: _kind, ...update } = input;
    const data = await gql<{ updateTranslationDeliveryTarget: DeliveryTarget }, { id: string; input: TargetInput }>(
      `mutation UpdateTarget($id: ID!, $input: TranslationDeliveryTargetUpdateInput!) {
        updateTranslationDeliveryTarget(id: $id, input: $input) { ${TARGET_FIELDS} }
      }`,
      { id, input: update },
    );
    return data.updateTranslationDeliveryTarget;
  }
  const data = await gql<{ createTranslationDeliveryTarget: DeliveryTarget }, { input: TargetInput }>(
    `mutation CreateTarget($input: TranslationDeliveryTargetInput!) {
      createTranslationDeliveryTarget(input: $input) { ${TARGET_FIELDS} }
    }`,
    { input },
  );
  return data.createTranslationDeliveryTarget;
}

export async function deleteTarget(id: string) {
  await gql<{ deleteTranslationDeliveryTarget: string }, { id: string }>(
    `mutation DeleteTarget($id: ID!) { deleteTranslationDeliveryTarget(id: $id) }`,
    { id },
  );
}

export async function publish(targetId?: string) {
  const data = await gql<{ publishTranslations: PublishReport[] }, { targetId?: string }>(
    `mutation Publish($targetId: ID) { publishTranslations(targetId: $targetId) { targetId name ok files error publishedAt } }`,
    { targetId },
  );
  return data.publishTranslations;
}

/** Storage presets offered when adding a delivery target. */
export interface Provider {
  id: string;
  label: string;
  kind: StorageKind;
  endpoint?: string;
  region?: string;
  forcePathStyle?: boolean;
  hint: string;
}

export const PROVIDERS: Provider[] = [
  {
    id: "garage",
    label: "Garage",
    kind: "S3",
    endpoint: "http://garage:3900",
    region: "garage",
    forcePathStyle: true,
    hint: "Self-hosted Garage: the S3 API port (3900) and a key created with `garage key create`.",
  },
  {
    id: "aws",
    label: "AWS S3",
    kind: "S3",
    region: "eu-west-1",
    hint: "Leave the endpoint empty. Leave both keys empty to use the server's IAM role.",
  },
  {
    id: "r2",
    label: "Cloudflare R2",
    kind: "S3",
    endpoint: "https://<account-id>.r2.cloudflarestorage.com",
    region: "auto",
    hint: "Create an R2 API token with Object Read & Write; replace <account-id> in the endpoint.",
  },
  {
    id: "minio",
    label: "MinIO",
    kind: "S3",
    endpoint: "http://minio:9000",
    region: "us-east-1",
    forcePathStyle: true,
    hint: "Any S3-compatible service works the same way.",
  },
  {
    id: "azure",
    label: "Azure Blob Storage",
    kind: "AZURE_BLOB",
    hint: "Storage account name + account key, or a container SAS token with create & write permissions.",
  },
];

/** Where a published file is read from (best effort, for links in the UI). */
export function publicFileUrl(target: DeliveryTarget, path: string) {
  if (!target.publicUrl) return null;
  return `${target.publicUrl.replace(/\/$/, "")}/${path}`;
}

/** Live URL of a served file, e.g. /i18n/json/fr.json. */
export function liveUrl(slug: string, path: string) {
  return `${window.location.origin}/i18n/${slug}/${path}`;
}
