// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import vikeReact from "vike-react/config";
import type { Config } from "vike/types";

export default {
  title: "CMS blog (Vike)",
  // Every page is rendered to static HTML at build time.
  prerender: true,
  extends: [vikeReact],
} satisfies Config;
