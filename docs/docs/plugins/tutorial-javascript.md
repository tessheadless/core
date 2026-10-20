---
sidebar_position: 6
title: "Tutorial: JavaScript"
---

# Writing a plugin in JavaScript

The [Extism JS PDK](https://github.com/extism/js-pdk) compiles JavaScript to
WebAssembly. The example `examples/plugins/js-moderation` trims every string
field on save, enforces a maximum title length, and refuses to publish
entries whose configured fields are empty.

:::note
This example is not compiled by the project's test suite, which would need
the `extism-js` toolchain. The Rust and Go examples are built and tested.
:::

## Code

`index.js`:

```js
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
```

`index.d.ts` declares the exports for the compiler:

```ts
declare module "main" {
  export function manifest(): I32;
  export function before_write(): I32;
}
```

## Build

Install the `extism-js` compiler (see the PDK's README), then:

```bash
extism-js index.js -i index.d.ts -o moderation.wasm
```

## Configure

```json
{ "requiredOnPublish": ["excerpt", "author"], "maxTitleLength": 120 }
```

A `publish` with an empty `excerpt` is rejected with
`cannot publish, missing: excerpt`.

On content types with the [review workflow](../developers/workflow.md),
entries are published by `approve` rather than `publish`. To enforce the
check there too, test `input.action === "approve"` as well. To catch problems
before reviewers see them, also test `"submit"`.
