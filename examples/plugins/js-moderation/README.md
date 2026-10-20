# js-moderation (JavaScript)

Trims string fields on save, enforces a maximum title length, and refuses to
publish entries whose configured fields are empty.

```sh
# Install the Extism JS PDK compiler: https://github.com/extism/js-pdk#install-the-compiler
extism-js index.js -i index.d.ts -o moderation.wasm
```

Config example: `{"requiredOnPublish": ["excerpt", "author"], "maxTitleLength": 120}`

> This example is not compiled by the repository's test script (it needs the
> `extism-js` toolchain); the Rust and Go examples are.
