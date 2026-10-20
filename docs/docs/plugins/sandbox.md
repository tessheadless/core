---
sidebar_position: 3
title: Sandbox and limits
---

# Sandbox and limits

Plugins run inside a WebAssembly sandbox (Extism on Wasmtime, WASI enabled):

| | Default per plugin | Operator maximum |
|---|---|---|
| Filesystem, environment variables | none | n/a |
| Outgoing HTTP | none | only to the plugin's **allowed hosts**; never if `TESS_PLUGIN_ALLOW_HTTP=false` |
| Timeout | 1000 ms | `TESS_PLUGIN_MAX_TIMEOUT_MS` (10 000) |
| Memory | 256 pages (16 MiB) | `TESS_PLUGIN_MAX_MEMORY_PAGES` (1024 = 64 MiB) |
| Module size | n/a | `TESS_PLUGIN_MAX_MODULE_BYTES` (20 MiB) |

- **Allowed hosts** are host names such as `hooks.example.com`, optionally
  with a port, and wildcards like `*.example.com`. URLs and schemes are
  rejected.
- **Integrity**: modules are stored in the media store under `plugins/`.
  Their SHA-256 checksum is recorded at install time and verified at every
  load.
- **Configuration** is plain JSON given to every call. Don't put secrets you
  can't afford to show to other Admins there.

## Performance

Plugins run **inside the request** that triggered them: a slow
`before_write` slows down every save. Keep `before_write` fast and local, and
do slow work (HTTP calls) in `after_write`, ideally delegating to a webhook
receiver. Each plugin instance processes one call at a time.

## Installing third-party plugins

Only Admins and Owners can install plugins, and they're installed
**disabled**. Before enabling one:

- read its source or trust its publisher;
- check which hosts it asks you to allow;
- scope it to the content types it needs;
- keep **fail open** off for validation plugins, so a broken plugin blocks
  writes rather than letting invalid data through.
