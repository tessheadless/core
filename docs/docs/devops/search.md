---
sidebar_position: 6
title: Search
---

# Search

Full-text search is optional. With `TESS_SEARCH=none` (the default), the
`search<Plural>` queries are simply not generated.

## Typesense

```bash
TESS_SEARCH=typesense
TYPESENSE_URL=http://typesense:8108
TYPESENSE_API_KEY=…              # admin key, from your secrets manager
TYPESENSE_COLLECTION_PREFIX=tess_
```

- **Opt-in per model**: an Admin (or the Owner) enables search on each
  content type in **Admin → Search** and picks the indexed fields. A model
  needs at least one selected `TEXT` or `MARKDOWN` field; only enabled models
  get a `search<Plural>` query.
- **Collections**: Tess creates one collection per enabled content type
  (`<prefix><ApiId>`). Saving the settings rebuilds it from the database;
  disabling search drops it.
- **Field schema**: generated from each field's type (the admin page shows
  the result):

  | Field type | Typesense field | Notes |
  |---|---|---|
  | `TEXT`, `MARKDOWN` | `string` | Searched by queries |
  | `ENUM`, `UUID`, single `RELATION` | `string`, facet | Exact value; relations hold entry UUIDs |
  | `RELATION` (many) | `string[]`, facet | |
  | `INT` | `int64`, sortable | |
  | `FLOAT` | `float`, sortable | |
  | `BOOLEAN` | `bool`, facet | |
  | `DATE`, `DATETIME` | `int64`, sortable | Unix seconds (`DATE` at midnight UTC) |

  `JSON`, components and dynamic zones can't be indexed, and passwords never
  are. Every collection also has a `stage` facet (`draft` / `published`).
- **What gets indexed**: the draft and published versions are separate
  documents (`draft_<uuid>`, `published_<uuid>`), so published content is
  never mixed with drafts.
- **When indexing happens**: on every create, update, publish, unpublish and
  delete, inside the same request. Changing a content type rebuilds its
  collection; fields removed from the model leave the index.
- **Results** are loaded from the database, so they always match stored
  content.

To share one Typesense cluster between installs, give each install its own
prefix.

### Reindexing

After restoring a backup, pointing Tess at a new Typesense cluster, or if
the index looks out of date, rebuild it with an Admin token:

```graphql
mutation { reindexSearch }                 # every model with search enabled
mutation { reindexSearch(apiId: "Post") }  # one type
```

The mutation returns the number of entries indexed. The admin page has a
**Reindex** button per model, and the settings are available through the API:

```graphql
query { searchSettings { engine models { apiId enabled active fields { name type selected indexType engineConfig } } } }
mutation { updateSearchSettings(apiId: "Post", enabled: true, fields: ["title", "body", "category"]) { active indexedEntries } }
```

## Memory

`TESS_SEARCH=memory` keeps a naive index in the process. It's handy for tests
and demos, but it's empty after every restart (run `reindexSearch`), and each
instance has its own. The per-model settings are kept in the database.
