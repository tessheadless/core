// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { Config } from "vike/types";
import vikeReact from "vike-react/config";

// Static site: pages are rendered to HTML at build time (SEO, no server),
// from the content stored in Tess.
const config: Config = {
  title: "Tess",
  ssr: true,
  prerender: true,
  extends: [vikeReact],
};

export default config;
