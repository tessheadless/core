// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { Config } from "vike/types";
import vikeReact from "vike-react/config";

// Default config (can be overridden by pages)
// https://vike.dev/config

const config: Config = {
  // https://vike.dev/head-tags
  title: "Tess Admin",
  description: "Administration interface of the headless CMS",

  // Static SPA: no Node server needed, so the same build works behind the
  // Rust server (Docker) or S3/CloudFront (AWS Lambda deployments).
  ssr: false,
  prerender: true,

  extends: [vikeReact],
};

export default config;
