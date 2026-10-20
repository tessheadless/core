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

- **Collections**: Tess creates one collection per content type
  (`<prefix><ApiId>`).
- **What gets indexed**: the draft and published versions are indexed as
  separate documents (published content is never mixed with drafts). Indexed
  fields are those of type `TEXT`, `MARKDOWN`, `DATE`, `INT`, `FLOAT`,
  `BOOLEAN` and `ENUM`; queries search the `TEXT` and `MARKDOWN` fields.
  Passwords are never indexed.
- **When indexing happens**: on every change, inside the same request.
  Changing a content type rebuilds its collection.
- **Searchable types**: only content types with at least one `TEXT` or
  `MARKDOWN` field get a `search<Plural>` query.
- **Results** are loaded from the database, so they always match stored
  content.

To share one Typesense cluster between installs, give each install its own
prefix.

### Reindexing

After restoring a backup, pointing Tess at a new Typesense cluster, or if
the index looks out of date, rebuild it with an Admin token:

```graphql
mutation { reindexSearch }                 # all content types
mutation { reindexSearch(apiId: "Post") }  # one type
```

The mutation returns the number of entries indexed.

## Memory

`TESS_SEARCH=memory` keeps a naive index in the process. It's handy for tests
and demos, but it's empty after every restart, and each instance has its own.
