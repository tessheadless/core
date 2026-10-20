---
sidebar_position: 8
title: Ids and refs
---

# Ids and refs

Tess never exposes database ids. Every entry has two identifiers instead:

| | `id` | `ref` |
|---|---|---|
| Looks like | `AXuW71Xk…` (long) | `4_vBlX0sxaaBG_AdKSON2g` |
| Stable | **no**: different in every response | **yes** |
| Expires | yes, after `TESS_ID_TTL` (24 h by default) | never |
| Accepted as input | yes: `post(id:)`, mutations, relations, `where: { id… }` | no |
| Use it for | an immediate follow-up call | React/Vue keys, caches, deduplication |

Ids are **encrypted and bound to their type**. An `Author` id passed where a
`Post` id is expected is rejected (`BAD_USER_INPUT`). After expiry they're
rejected with `ID_EXPIRED`: query the entry again to get a fresh id.

## Practical rules

- **Route by your own fields**, typically a `slug`:
  `posts(where: { slug: $slug }, first: 1)`. Never put an `id` in a URL.
- **Use `ref`** for list keys and cache keys, and to recognize the same entry
  across responses.
- **Never store `id`s** in a database, a static build or `localStorage`.
- **Relations** take ids: fetch the related entries, then write right away.
- Plugins receive `ref` too, never ids.
