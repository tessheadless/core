// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { ApiError, authHeader, gql, handleAuthFailure } from "./graphql";

export type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue };

export interface Plugin {
  name: string;
  version: string | null;
  description: string | null;
  enabled: boolean;
  loaded: boolean;
  lastError: string | null;
  exports: string[];
  contentTypes: string[];
  allowedHosts: string[];
  config: JsonValue;
  timeoutMs: number;
  memoryMaxPages: number;
  failOpen: boolean;
  sha256: string;
  size: number;
  installedAt: string;
  updatedAt: string;
}

export interface PluginLimits {
  maxModuleBytes: number;
  maxTimeoutMs: number;
  maxMemoryPages: number;
  allowHttp: boolean;
}

/** Omitted fields are left unchanged by the server. */
export interface PluginSettingsInput {
  enabled?: boolean;
  config?: JsonValue;
  contentTypes?: string[];
  allowedHosts?: string[];
  timeoutMs?: number;
  memoryMaxPages?: number;
  failOpen?: boolean;
}

export const PLUGIN_NAME_PATTERN = /^[a-z][a-z0-9-]{1,47}$/;

const PLUGIN_FIELDS = `
  name version description enabled loaded lastError exports contentTypes allowedHosts
  config timeoutMs memoryMaxPages failOpen sha256 size installedAt updatedAt
`;

export interface PluginsPageData {
  plugins: Plugin[];
  pluginLimits: PluginLimits;
  contentTypes: { apiId: string }[];
}

export function fetchPluginsPage() {
  return gql<PluginsPageData>(`
    query PluginsPage {
      plugins { ${PLUGIN_FIELDS} }
      pluginLimits { maxModuleBytes maxTimeoutMs maxMemoryPages allowHttp }
      contentTypes { apiId }
    }
  `);
}

export async function updatePlugin(name: string, input: PluginSettingsInput): Promise<Plugin> {
  const data = await gql<{ updatePlugin: Plugin }, { name: string; input: PluginSettingsInput }>(
    `mutation UpdatePlugin($name: String!, $input: PluginSettingsInput!) {
      updatePlugin(name: $name, input: $input) { ${PLUGIN_FIELDS} }
    }`,
    { name, input },
  );
  return data.updatePlugin;
}

export async function uninstallPlugin(name: string): Promise<string> {
  const data = await gql<{ uninstallPlugin: string }, { name: string }>(
    `mutation UninstallPlugin($name: String!) { uninstallPlugin(name: $name) }`,
    { name },
  );
  return data.uninstallPlugin;
}

/**
 * Install or replace a plugin by streaming the raw .wasm module to the REST endpoint.
 * Success: the plugin JSON. Failure: `{"error": "...", "code": "..."}` with a 4xx/5xx status.
 */
export async function uploadPlugin(name: string, file: File): Promise<Plugin> {
  let res: Response;
  try {
    res = await fetch(`/api/plugins/${encodeURIComponent(name)}`, {
      method: "POST",
      body: file,
      headers: { ...authHeader(), "Content-Type": "application/wasm" },
    });
  } catch (error) {
    if (error instanceof ApiError) throw error;
    throw new ApiError("Cannot reach the CMS server.", "NETWORK_ERROR");
  }

  let body: unknown = null;
  try {
    body = await res.json();
  } catch {
    // Non-JSON body (proxy error page…), handled below.
  }

  if (!res.ok) {
    const err = (body ?? {}) as { error?: unknown; code?: unknown };
    const code = typeof err.code === "string" ? err.code : undefined;
    handleAuthFailure(res.status, code, false);
    const message = typeof err.error === "string" ? err.error : `Upload failed (HTTP ${res.status}).`;
    throw new ApiError(message, code ?? (res.status === 401 ? "FORBIDDEN" : undefined), res.status);
  }
  if (!body || typeof body !== "object")
    throw new ApiError("Unexpected response from the server.", undefined, res.status);
  return body as Plugin;
}

/** Quick client-side sanity check: every WebAssembly binary starts with "\0asm". */
export async function looksLikeWasm(file: File): Promise<boolean> {
  const head = new Uint8Array(await file.slice(0, 4).arrayBuffer());
  return head.length === 4 && head[0] === 0x00 && head[1] === 0x61 && head[2] === 0x73 && head[3] === 0x6d;
}

/** Suggest a valid plugin name from a file name, e.g. "Rust_Slugify.wasm" -> "rust-slugify". */
export function suggestName(fileName: string): string {
  return fileName
    .replace(/\.wasm$/i, "")
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "-")
    .replace(/^[^a-z]+/, "")
    .replace(/-+$/, "")
    .slice(0, 48);
}
