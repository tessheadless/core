# Plugin examples

Plugins extend the CMS without rebuilding it. Each one is a WebAssembly module
built with an [Extism PDK](https://extism.org/docs/concepts/pdk) (Rust, Go,
JavaScript, Python, Zig, C#, ...), uploaded by an admin in **Admin → Plugins**
(or `POST /api/plugins/{name}`), then enabled and configured there.

| Example | Language | Hook | What it does | Built by `scripts/test-all.sh` |
|---|---|---|---|---|
| [`rust-slugify`](rust-slugify) | Rust | `before_write` | fills `slug` from `title`, rejects forbidden words | yes |
| [`rust-webhook`](rust-webhook) | Rust | `after_write` + HTTP | notifies a webhook on publish | yes |
| [`go-reading-time`](go-reading-time) | Go (wasip1) | `before_write` | computes a reading time | yes |
| [`js-moderation`](js-moderation) | JavaScript | `before_write` | trims text, checks fields before publishing | no (needs `extism-js`) |

## Contract

A plugin exports any of these functions. All inputs and outputs are JSON.

### `manifest` (optional)
Output: `{"version": "1.0.0", "description": "..."}`, shown in the admin.

### `before_write`
Called before `create`, `update`, `publish`, `delete` and `unpublish` are
saved. For content types with the review workflow, it is also called for
`submit`, `approve` and `requestChanges`. It receives the normalized draft
data and who is acting:

```json
{"action": "create", "contentType": "Post", "data": {"title": "Hello"},
 "actor": {"name": "Ada Example", "role": "CONTRIBUTOR"}, "config": {}}
```

Return one of:
- `{}`: no change;
- `{"data": {...}}`: replace the data, which is validated again against the
  content type. This is only honored for `create`, `update`, `publish` and
  `approve`.
- `{"error": "message"}`: reject the write; the message is returned to the
  API client with code `BAD_USER_INPUT`.

If the plugin crashes or times out, the write is rejected with code
`PLUGIN_ERROR`, unless **fail open** is enabled for that plugin.

### `after_write`
Called after a change has been saved. Its output is ignored and failures never
affect the write; they are shown as the plugin's last error in the admin.

```json
{
  "action": "publish",
  "contentType": "Post",
  "entry":    {"ref": "jb56dtysPiTvASjGtgm8Gg", "draft": {...}, "published": {...},
               "createdAt": "...", "updatedAt": "...", "publishedAt": "..."},
  "previous": {...},
  "actor": {"name": "Rita Reviewer"},
  "config": {}
}
```

`entry` is `null` after a delete. Entries are identified by their stable
`ref`; internal database ids are never given to plugins.

## Sandbox

- No filesystem and no environment variables.
- No network unless the admin lists **allowed hosts** for the plugin (wildcards
  like `*.example.com` are accepted). The server operator can forbid network
  access for all plugins with `TESS_PLUGIN_ALLOW_HTTP=false`.
- A per-plugin timeout (default 1 s) and memory cap (default 16 MiB), bounded by
  `TESS_PLUGIN_MAX_TIMEOUT_MS` / `TESS_PLUGIN_MAX_MEMORY_PAGES`.
- **Scope**: a plugin applies to all content types, or only to the ones
  selected in the admin.
- Plugins are installed **disabled**; modules are stored in the media store
  with their SHA-256 checksum, verified when loaded.

Plugins run inside the request that triggered them, on every instance
(containers or Lambda environments pick up changes within
`TESS_SCHEMA_REFRESH_SECS`). Keep them fast; use `after_write` + a webhook for
slow work.
