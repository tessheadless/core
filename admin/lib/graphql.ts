// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { clearToken, getToken, SESSION_EXPIRED_MESSAGE } from "./auth";

/** Error codes returned by the CMS in `extensions.code` (plus client-side ones). */
export type ErrorCode =
  | "FORBIDDEN"
  | "BAD_USER_INPUT"
  | "NOT_FOUND"
  | "CONFLICT"
  | "ID_EXPIRED"
  | "PLUGIN_ERROR"
  | "INTERNAL_SERVER_ERROR"
  | "NETWORK_ERROR"
  | (string & {});

export class ApiError extends Error {
  readonly code: ErrorCode | undefined;
  readonly status: number | undefined;

  constructor(message: string, code?: ErrorCode, status?: number) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.status = status;
  }
}

interface GraphQLErrorPayload {
  message: string;
  extensions?: { code?: string };
}

interface GraphQLResponse<TData> {
  data?: TData | null;
  errors?: GraphQLErrorPayload[];
}

interface RequestOptions {
  /** Use this token instead of the stored one (sign-in, setup). Auth failures then don't sign out. */
  token?: string;
  /** Send no Authorization header at all (`setupRequired`, `login`). */
  anonymous?: boolean;
}

/**
 * Signs out when the server rejects the stored credential. Invalid, expired or
 * revoked credentials get HTTP 401 (`UNAUTHENTICATED` on REST routes); FORBIDDEN
 * only means the role may not do this, so it must not sign the user out.
 * Returns true when the failure was an auth failure.
 */
export function handleAuthFailure(status: number | undefined, code: string | undefined, explicitToken: boolean) {
  const isAuthFailure = status === 401 || code === "UNAUTHENTICATED";
  if (isAuthFailure && !explicitToken) clearToken(SESSION_EXPIRED_MESSAGE);
  return isAuthFailure;
}

export function authHeader(explicitToken?: string): Record<string, string> {
  const token = explicitToken ?? getToken();
  if (!token) {
    if (explicitToken === undefined) clearToken(SESSION_EXPIRED_MESSAGE);
    throw new ApiError("Not signed in", "FORBIDDEN", 401);
  }
  return { Authorization: `Bearer ${token}` };
}

async function readJson(res: Response): Promise<unknown> {
  try {
    return await res.json();
  } catch {
    return null;
  }
}

/** POST a GraphQL operation to /graphql and return `data`, or throw an ApiError. */
export async function gql<TData, TVars extends Record<string, unknown> = Record<string, never>>(
  query: string,
  variables?: TVars,
  options: RequestOptions = {},
): Promise<TData> {
  const explicitToken = options.token !== undefined || options.anonymous === true;
  const auth = options.anonymous ? {} : authHeader(options.token);
  let res: Response;
  try {
    res = await fetch("/graphql", {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json", ...auth },
      body: JSON.stringify({ query, variables: variables ?? {} }),
    });
  } catch {
    throw new ApiError("Cannot reach the CMS server.", "NETWORK_ERROR");
  }

  const body = (await readJson(res)) as GraphQLResponse<TData> | null;
  const errors = body?.errors ?? [];
  if (errors.length > 0) {
    const code =
      errors.find((e) => e.extensions?.code)?.extensions?.code ?? (res.status === 401 ? "UNAUTHENTICATED" : undefined);
    handleAuthFailure(res.status, code, explicitToken);
    throw new ApiError(errors.map((e) => e.message).join("; "), code, res.status);
  }
  if (!res.ok) {
    handleAuthFailure(res.status, undefined, explicitToken);
    throw new ApiError(
      res.status === 401 ? "Invalid or expired credentials." : `Request failed (HTTP ${res.status}).`,
      res.status === 401 ? "UNAUTHENTICATED" : undefined,
      res.status,
    );
  }
  if (body?.data == null) throw new ApiError("Empty response from the server.", undefined, res.status);
  return body.data;
}

/** Human-readable message for toasts and alerts, e.g. "Name taken (CONFLICT)". */
export function describeError(error: unknown): string {
  if (error instanceof ApiError) return error.code ? `${error.message} (${error.code})` : error.message;
  if (error instanceof Error) return error.message;
  return "Unexpected error.";
}
