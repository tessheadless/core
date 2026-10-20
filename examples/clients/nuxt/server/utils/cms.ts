// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// Server-side GraphQL helper (auto-imported in server/ by Nitro).
type GraphQLResponse<T> = { data?: T; errors?: { message: string; extensions?: { code?: string } }[] };

const STATUS_BY_CODE: Record<string, number> = { BAD_USER_INPUT: 400, FORBIDDEN: 403, NOT_FOUND: 404, ID_EXPIRED: 410 };

export async function cmsQuery<T>(query: string, variables: Record<string, unknown> = {}): Promise<T> {
  const { cmsUrl } = useRuntimeConfig();
  const res = await $fetch<GraphQLResponse<T>>(`${cmsUrl.replace(/\/+$/, "")}/graphql`, {
    method: "POST",
    body: { query, variables },
  });
  if (res.errors?.length) {
    const [e] = res.errors;
    const code = e.extensions?.code ?? "UNKNOWN";
    throw createError({ statusCode: STATUS_BY_CODE[code] ?? 502, statusMessage: code, message: e.message });
  }
  return res.data as T;
}

export const POST_CARD = "ref title slug excerpt publishedAt author { name }";
