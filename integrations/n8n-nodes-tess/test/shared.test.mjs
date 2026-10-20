// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import assert from "node:assert/strict";
import { createHmac } from "node:crypto";
import { test } from "node:test";
import shared from "../dist/nodes/shared.js";

const { verifySignature, rewriteOrigin, names, defaultSelection } = shared;

test("signature verification matches the Tess server", () => {
  const body = Buffer.from('{"event":"entry.publish"}');
  const t = Math.floor(Date.now() / 1000);
  const v1 = createHmac("sha256", "s3cret").update(`${t}.`).update(body).digest("hex");
  assert.ok(verifySignature("s3cret", `t=${t},v1=${v1}`, body));
  assert.ok(!verifySignature("other", `t=${t},v1=${v1}`, body));
  assert.ok(!verifySignature("s3cret", `t=${t},v1=${v1}`, Buffer.from("{}")));
  assert.ok(!verifySignature("s3cret", `t=${t - 3600},v1=${v1}`, body), "too old");
  assert.ok(!verifySignature("s3cret", undefined, body));
});

test("helpers", () => {
  assert.equal(rewriteOrigin("https://n8n.example.com/n8n/webhook/abc/webhook", "http://n8n:5678"), "http://n8n:5678/n8n/webhook/abc/webhook");
  assert.equal(rewriteOrigin("https://a/b", ""), "https://a/b");
  const t = { apiId: "BlogPost", pluralApiId: "blogPosts", workflow: false, fields: [{ name: "title", type: "TEXT" }, { name: "author", type: "RELATION" }] };
  assert.equal(names(t).single, "blogPost");
  assert.equal(names(t).create, "createBlogPost");
  assert.ok(defaultSelection(t).endsWith("workflowStatus title"));
});
