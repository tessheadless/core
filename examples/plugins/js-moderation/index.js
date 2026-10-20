// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// Example CMS plugin in JavaScript: blocks publishing until required fields
// are filled, and trims every string field on save.
//
// Config: {"requiredOnPublish": ["excerpt", "author"], "maxTitleLength": 120}
//
// Build with the Extism JS PDK (https://github.com/extism/js-pdk):
//   extism-js index.js -i index.d.ts -o moderation.wasm

function manifest() {
  Host.outputString(
    JSON.stringify({ version: "1.0.0", description: "Trims text and checks fields before publishing." }),
  );
}

function before_write() {
  const input = JSON.parse(Host.inputString());
  const config = input.config || {};
  const data = input.data || {};

  if (input.action === "create" || input.action === "update") {
    for (const [key, value] of Object.entries(data)) {
      if (typeof value === "string") data[key] = value.trim();
    }
    const max = config.maxTitleLength;
    if (max && typeof data.title === "string" && data.title.length > max) {
      Host.outputString(JSON.stringify({ error: `title is longer than ${max} characters` }));
      return;
    }
    Host.outputString(JSON.stringify({ data }));
    return;
  }

  if (input.action === "publish") {
    const missing = (config.requiredOnPublish || []).filter(
      (field) => data[field] === undefined || data[field] === null || data[field] === "",
    );
    if (missing.length > 0) {
      Host.outputString(JSON.stringify({ error: `cannot publish, missing: ${missing.join(", ")}` }));
      return;
    }
  }
  Host.outputString("{}");
}

module.exports = { manifest, before_write };
