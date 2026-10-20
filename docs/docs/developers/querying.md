---
sidebar_position: 3
title: Filtering, sorting, pagination
---

# Filtering, sorting and pagination

```graphql
{
  posts(
    where: { title_contains: "rust", OR: [{ views_gt: 100 }, { featured: true }] }
    orderBy: [publishedAt_DESC, title_ASC]
    first: 10
    skip: 20
  ) {
    total
    items { ref title views publishedAt }
  }
}
```

## `where`

Filters are named `<field>_<operator>`. Plain `<field>` means equality.

| Field kind | Operators |
|---|---|
| `TEXT`, `MARKDOWN` | `field`, `_not`, `_in`, `_not_in`, `_contains`, `_not_contains`, `_starts_with`, `_ends_with` |
| `INT`, `FLOAT`, `DATETIME`, `DATE` | `field`, `_not`, `_in`, `_not_in`, `_lt`, `_lte`, `_gt`, `_gte` |
| `BOOLEAN` | `field`, `_not` |
| `ENUM`, `UUID`, single `RELATION` (by id) | `field`, `_not`, `_in`, `_not_in` |
| System fields | `id`, `createdAt`, `updatedAt`, `publishedAt` (like `DATETIME`), `workflowStatus` (like `ENUM`) |

`JSON`, `PASSWORD`, many-relations, components and dynamic zones aren't
filterable. Combine conditions with `AND`,
`OR` and `NOT`, which take lists of `where` inputs (`NOT` matches when none
of its conditions do).

Semantics, identical on every database:

- **Missing values** behave like `null`. `field: null` matches entries
  without a value, and `field_not: null` matches entries with one.
- **Comparisons** only match values of the same type.
- **String matching** (`_contains`, `_starts_with`, `_ends_with`) and string
  ordering are case-sensitive and bytewise.
- **Date-times** in filters may use any RFC 3339 offset; they are normalized
  to UTC.

## `orderBy`

A list of `<field>_ASC` / `<field>_DESC` values, in priority order. Sortable
fields: `id`, `createdAt`, `updatedAt`, `publishedAt`, and the content
fields of type `TEXT`, `MARKDOWN`, `INT`, `FLOAT`, `BOOLEAN`, `DATETIME`, `DATE` and
`ENUM`. The default order is `createdAt_ASC`. Ties are always broken by id,
so pagination is stable. Missing values sort first in ascending order.

## Pagination

- **`first`**: page size. It defaults to `TESS_DEFAULT_PAGE_SIZE` (100) and
  is capped at `TESS_MAX_PAGE_SIZE` (1000).
- **`skip`**: offset.
- **`total`**: the number of entries matching the filter, regardless of the
  page.

Queries deeper than `TESS_MAX_QUERY_DEPTH` (16) are rejected.
