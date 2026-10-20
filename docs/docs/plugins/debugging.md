---
sidebar_position: 8
title: Debugging
---

# Debugging plugins

## Where errors show up

- **`lastError`** on the plugin, in the admin or with `plugins { name lastError }`.
- **Server logs** at `warn` level (`plugin failure`) and `error` level
  (`cannot load plugin`).
- **API errors** for `before_write`: `BAD_USER_INPUT` when your plugin returns
  `{"error": …}`, `PLUGIN_ERROR` when it fails.

## Common problems

| Symptom | Cause and fix |
|---|---|
| Install fails: "exports none of [before_write, after_write]" | The function isn't exported. Check `#[plugin_fn]` (Rust), `//go:wasmexport` (Go), or `module.exports` and `index.d.ts` (JS). |
| Install fails: "not a WebAssembly module" | You uploaded the wrong file (e.g. the Rust `.rlib`). Use the `.wasm` from `target/wasm32-unknown-unknown/release/`. |
| `PLUGIN_ERROR … returned invalid JSON` | `before_write` must output a JSON object, or nothing. |
| `HTTP request to … is not allowed` | Add the host to **allowed hosts**, and check the operator didn't set `TESS_PLUGIN_ALLOW_HTTP=false`. |
| Timeouts | Raise `timeoutMs` (up to the operator maximum), or move slow work to `after_write`. |
| `BAD_USER_INPUT: unknown field …` after your plugin ran | The plugin set a field the content type doesn't have. Scope it to the right content types. |
| Changes don't apply | Is the plugin `enabled`, and is the content type in its scope? Other instances apply changes within `TESS_SCHEMA_REFRESH_SECS`. |

## Testing locally

1. Start a throwaway server: `scripts/dev-server.sh` (in-memory, port 8080).
2. Upload and enable your module with `curl`, as in
   [Managing plugins](managing.md).
3. Trigger it with GraphQL mutations, and read `plugins { lastError }`.

To test a module without a server, call it with a sample input. For example,
with the [Extism CLI](https://github.com/extism/cli):

```bash
extism call slugify_plugin.wasm before_write --wasi \
  --input '{"action":"create","contentType":"Post","data":{"title":"Hello"},"config":{}}'
```

For automated tests in Rust, `crates/plugins/tests/wasm.rs` shows how to load
a module through the plugin manager against an in-memory store.

## Versioning

Return a `version` from `manifest`, and bump it with each release; the admin
shows it. Replacing a module keeps its settings, so make new config keys
optional.
