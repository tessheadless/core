// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { gql } from "./graphql";

export type IndexType = "TEXT" | "KEYWORD" | "KEYWORDS" | "INT" | "FLOAT" | "BOOL" | "TIMESTAMP";

export interface SearchField {
  name: string;
  /** Field type, e.g. `TEXT`. */
  type: string;
  selected: boolean;
  /** False for JSON, components, dynamic zones and passwords. */
  supported: boolean;
  indexType: IndexType | null;
  /** Matched by search queries. */
  searchable: boolean;
  /** Field definition generated for the engine (Typesense); null for the memory engine. */
  engineConfig: Record<string, unknown> | null;
}

export interface SearchModel {
  apiId: string;
  enabled: boolean;
  /** Enabled with a full-text field: the `search<Plural>` query exists. */
  active: boolean;
  fields: SearchField[];
  /** Set by updates only. */
  indexedEntries: number | null;
}

export interface SearchSettings {
  engine: string;
  models: SearchModel[];
}

const MODEL =
  "apiId enabled active indexedEntries fields { name type selected supported indexType searchable engineConfig }";

/** `null` when the server has no search engine (TESS_SEARCH). */
export async function fetchSearchSettings() {
  const data = await gql<{ searchSettings: SearchSettings | null }>(
    `query SearchSettings { searchSettings { engine models { ${MODEL} } } }`,
  );
  return data.searchSettings;
}

/** Saves the settings and rebuilds the model's index. */
export async function updateSearchSettings(apiId: string, enabled: boolean, fields: string[]) {
  const data = await gql<{ updateSearchSettings: SearchModel }, { apiId: string; enabled: boolean; fields: string[] }>(
    `mutation UpdateSearchSettings($apiId: String!, $enabled: Boolean!, $fields: [String!]!) {
      updateSearchSettings(apiId: $apiId, enabled: $enabled, fields: $fields) { ${MODEL} }
    }`,
    { apiId, enabled, fields },
  );
  return data.updateSearchSettings;
}

export async function reindexSearch(apiId: string) {
  const data = await gql<{ reindexSearch: number }, { apiId: string }>(
    `mutation ReindexSearch($apiId: String!) { reindexSearch(apiId: $apiId) }`,
    { apiId },
  );
  return data.reindexSearch;
}
