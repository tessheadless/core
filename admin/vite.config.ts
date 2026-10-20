// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import vike from "vike/plugin";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const apiUrl = process.env.TESS_API_URL ?? "http://localhost:8080";

// The admin is built as static files and served by the Rust server under /admin/.
// In dev, API calls (GraphQL, REST plugin upload, media) are proxied to the Rust server (TESS_API_URL).
export default defineConfig({
  base: "/admin/",
  plugins: [vike(), react(), tailwindcss()],
  server: {
    proxy: {
      "/graphql": apiUrl,
      "/api": apiUrl,
      "/media": apiUrl,
      "/i18n": apiUrl,
      // The embedded n8n editor (proxied by the Rust server when TESS_N8N_URL is set).
      "/n8n": { target: apiUrl, ws: true },
    },
  },
});
