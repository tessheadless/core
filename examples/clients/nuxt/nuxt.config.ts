// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: "2025-07-15",
  runtimeConfig: {
    // Private (server-only). Build-time default from TESS_URL; can be
    // overridden at runtime with NUXT_CMS_URL.
    cmsUrl: process.env.TESS_URL || "http://localhost:8080",
  },
  nitro: {
    // `nuxt build` prerenders the home page and every /posts/<slug> it links
    // to (so the build fetches from the CMS); other slugs are rendered on demand.
    prerender: { routes: ["/"], crawlLinks: true },
  },
  routeRules: {
    // Cache the JSON proxy routes for 60 s (stale-while-revalidate).
    "/api/**": { swr: 60 },
  },
});
