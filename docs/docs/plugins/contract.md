---
sidebar_position: 2
title: The plugin contract
---

# The plugin contract

A plugin exports one or more of these functions. Inputs and outputs are JSON
strings.

| Export | Required | When |
|---|---|---|
| `manifest` | no | at install time, to read the version and description |
| `before_write` | one of the two | before a change is saved |
| `after_write` | one of the two | after a change is saved |

## `manifest`

No input. Output:

```json
{ "version": "1.0.0", "description": "Generates URL slugs." }
```

## `before_write`

Input:

```json
{
  "action": "create",
  "contentType": "Post",
  "data": { "title": "Hello world" },
  "actor": { "name": "Ada Example", "role": "CONTRIBUTOR" },
  "config": { "anything": "the admin configured" }
}
```

- **`action`**: one of `create`, `update`, `delete`, `publish`, `unpublish`,
  plus `submit`, `approve` and `requestChanges` on content types with the
  [review workflow](../developers/workflow.md).
- **`data`**: the entry's normalized draft, without system fields.

Return one of:

| Output | Effect |
|---|---|
| `{}` (or empty) | no change |
| `{"data": { … }}` | replace the data. Honored for `create`, `update`, `publish` and `approve`, and validated again against the content type. Ignored for other actions. |
| `{"error": "message"}` | reject the change. The API client gets `BAD_USER_INPUT` with your message. |

If the plugin **crashes, times out or returns invalid JSON**, the change is
rejected with `PLUGIN_ERROR`, unless the admin enabled **fail open** for
that plugin, in which case the error is logged and the change goes through.

## `after_write`

Input:

```json
{
  "action": "publish",
  "contentType": "Post",
  "entry": {
    "ref": "jb56dtysPiTvASjGtgm8Gg",
    "draft": { "title": "Hello world" },
    "published": { "title": "Hello world" },
    "createdAt": "2026-10-18T06:22:55.050Z",
    "updatedAt": "2026-10-18T06:22:55.050Z",
    "publishedAt": "2026-10-18T06:22:55.274Z",
    "workflowStatus": "PUBLISHED"
  },
  "previous": { "…": "the entry before the change, or null after a create" },
  "actor": { "name": "Rita Reviewer" },
  "config": {}
}
```

`entry` is `null` after a delete. The output is ignored. Failures never
affect the change; they're recorded as the plugin's `lastError` (visible in
the admin) and in the logs.

Entries are identified by their stable `ref`. Database ids are never given
to plugins.

## Ordering and scope

- **Order**: enabled plugins run in alphabetical order of their names, each
  seeing the previous one's output.
- **Content types**: a plugin runs for the content types selected by the
  admin, or for all of them when none are selected.
- **Validation**: data a plugin returns must still fit the content type.
  Setting a field the type doesn't have fails validation, so scope plugins to
  types that have the fields they write.
