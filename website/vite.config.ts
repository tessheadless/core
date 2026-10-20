// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import vike from "vike/plugin";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Static site: every page is prerendered at build time from the content
// stored in Tess (see lib/tess.ts). Deploy `dist/client/` to any static host.
export default defineConfig({
  plugins: [vike(), react(), tailwindcss()],
});
