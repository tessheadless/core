// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// n8n resolves `file:tess.svg` next to each compiled node and credential.
import { copyFileSync, mkdirSync } from "node:fs";

for (const dir of ["dist/nodes/Tess", "dist/nodes/TessTrigger", "dist/credentials"]) {
  mkdirSync(dir, { recursive: true });
  copyFileSync("icons/tess.svg", `${dir}/tess.svg`);
}
