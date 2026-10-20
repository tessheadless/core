---
sidebar_position: 7
title: Search
---

# Search

When the server has a search engine configured (see
[DevOps: search](../devops/search.md)), every content type with at least one
`TEXT` or `MARKDOWN` field gets a search query:

```graphql
{
  searchPosts(query: "graphql rust", first: 10, skip: 0) {
    total
    items { ref title excerpt }
  }
}
```

- **Ranking**: results are ordered by relevance; `total` is the number of
  matches.
- **What's searched**: the type's text fields. Typo tolerance and ranking
  come from the engine (Typesense).
- **Drafts**: `stage: DRAFT` searches drafts, which requires being signed
  in. Published and draft content are indexed separately.
- **Freshness**: entries are loaded from the database after the search, so
  you always get current data.
- **Page size**: `first` defaults to 20 and is capped at 250 (or
  `TESS_MAX_PAGE_SIZE` if lower).

If `searchPosts` isn't in the schema, search is disabled on that server, or
the type has no text field.
