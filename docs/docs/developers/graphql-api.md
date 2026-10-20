---
sidebar_position: 2
title: Content models and the generated API
---

# Content models and the generated API

## Defining content types

Content types are managed by Admins, in the admin UI or with these
mutations:

```graphql
mutation {
  createContentType(input: {
    apiId: "Post"                    # PascalCase: becomes the GraphQL type name
    pluralApiId: "posts"             # optional (defaults to camelCase + "s")
    description: "Blog posts"
    workflow: true                   # require review before publishing
    fields: [
      { name: "title",    type: TEXT, required: true }                 # max 80 characters by default
      { name: "slug",     type: TEXT, pattern: "[a-z0-9]+(?:-[a-z0-9]+)*" }
      { name: "contact",  type: TEXT, pattern: "[^@\\s]+@[^@\\s]+\\.[a-z]{2,}" }   # an email field
      { name: "excerpt",  type: TEXT, minLength: 20, maxLength: 300 }
      { name: "notes",    type: TEXT, maxLength: null }                 # unlimited
      { name: "body",     type: MARKDOWN }
      { name: "views",    type: INT }
      { name: "rating",   type: FLOAT }
      { name: "featured", type: BOOLEAN }
      { name: "date",     type: DATE }
      { name: "updated",  type: DATETIME }
      { name: "code",     type: UUID }
      { name: "pin",      type: PASSWORD, minLength: 12 }
      { name: "meta",     type: JSON }
      { name: "status",   type: ENUM, enumValues: ["NEWS", "TUTORIAL"] }
      { name: "author",   type: RELATION, relationTarget: "Author" }
      { name: "tags",     type: RELATION, relationTarget: "Tag", many: true }
      { name: "cover",    type: RELATION, relationTarget: "Asset" }
      { name: "seo",      type: COMPONENT, component: "Seo" }
      { name: "blocks",   type: DYNAMIC_ZONE, components: ["Hero", "Quote"] }
    ]
  }) { apiId pluralApiId workflow }
}
```

| Field type | GraphQL type | Notes |
|---|---|---|
| `TEXT` | `String` | `minLength` (default 0), `maxLength` (**default 80**; `null` for unlimited), `pattern` |
| `MARKDOWN` | `String` | Markdown or MDX source, rendered by your clients; [raw HTML is refused](#markdown) |
| `INT` / `FLOAT` | `Int` / `Float` | |
| `BOOLEAN` | `Boolean` | |
| `DATETIME` | `DateTime` | RFC 3339, normalized to UTC with milliseconds (`2026-01-31T12:00:00.000Z`) |
| `DATE` | `Date` | `YYYY-MM-DD`, no time |
| `UUID` | `UUID` | normalized to lowercase; **generated** when left empty on create |
| `PASSWORD` | input only | `minLength` (default 8); [hashed, never returned](#passwords) |
| `JSON` | `JSON` | any JSON value |
| `ENUM` | `<Type><Field>Enum` | |
| `RELATION` | the target type, or `[Target!]!` with `many` | |
| `COMPONENT` | the component type, or `[Component!]!` with `repeatable` | see [Components](components.md) |
| `DYNAMIC_ZONE` | `[<Type><Field>Zone!]!` (a union) | see [Components](components.md) |

### Text formats

`pattern` is a regular expression in [Rust `regex`](https://docs.rs/regex)
syntax, and the **whole value** must match it. Examples:

| Format | `pattern` |
|---|---|
| Email | `[^@\s]+@[^@\s]+\.[A-Za-z]{2,}` |
| URL | `https?://\S+` |
| Slug | `[a-z0-9]+(?:-[a-z0-9]+)*` |
| Phone | `\+[1-9][0-9 ]{6,18}` |

Lookarounds and backreferences aren't supported, which guarantees
matching in linear time. Patterns are limited to 500 characters.

### Markdown

`MARKDOWN` fields keep the source as-is; render it in your clients with
the Markdown or MDX library of your choice.

- **Allowed (MDX):** components, because they start with an uppercase letter
  (`<Callout>`, `<Chart data={…} />`, `</Callout>`), as well as `{…}`
  expressions and fragments (`<>`).
- **Allowed (Markdown):** autolinks such as `<https://…>` and
  `<ada@example.com>`.
- **Rejected (HTML):** lowercase tags (`<div>`, `<script>`, `<img …>`),
  comments `<!-- -->` and `<!DOCTYPE>`. The error names the tag.
- **Code is exempt:** anything inside fenced code blocks and inline code
  spans is ignored, so documentation can show HTML examples.

### Passwords

`PASSWORD` values are hashed with Argon2id when written; the plain text is
never stored. The field isn't part of the output type, filters, sorting,
search or plugin inputs (plugins only see the hash). To check a candidate,
for example for a password-protected page:

```graphql
query($id: ID!) {
  verifyPassword(contentType: "Page", id: $id, field: "pin", password: "…")   # → true / false
}
```

`verifyPassword` reads the `PUBLISHED` stage by default, and has the same
access rules as reading the entry. Password fields are only allowed
directly in content types, not inside components.

**Naming rules**:

- **`apiId`**: PascalCase, letters and digits only.
- **Field names**: camelCase, letters and digits only, at most 64 characters.
- **API IDs are shared** between content types and
  [components](components.md).
- **Reserved field names**: `id`, `ref`, `stage`, `createdAt`, `updatedAt`,
  `publishedAt`, `workflowStatus`, `createdBy`, `updatedBy`, `history`,
  `where` and `orderBy`.
- **Collisions**: names that would collide in the generated schema are
  rejected with `CONFLICT`.

Other mutations:

- **`updateContentType(apiId, input)`** replaces the whole definition. Send
  every field you want to keep, and `workflow` too: omitting it turns the
  workflow off. Renaming isn't supported. Data of removed fields is hidden,
  then dropped on the next write.
- **`deleteContentType(apiId)`** deletes the type **and all its entries**.
  It's refused while other types still have relations to it.
- **`contentTypes`** and **`contentType(apiId)`** list the models. Unlike the
  mutations (Owner/Admin), they are readable by every signed-in user or token
  (the admin content manager builds its forms from them), not by anonymous
  clients. The same goes for `components` and `component(apiId)`.

The built-in `Asset` type (media) cannot be modified.

## What is generated

For a type `Post` with plural `posts`:

| Operation | Purpose |
|---|---|
| `post(id: ID!, stage: Stage = PUBLISHED): Post` | one entry |
| `posts(where, orderBy, first, skip, stage): PostConnection!` | a list: `{ total items }` |
| `searchPosts(query: String!, first, skip, stage)` | full-text search, when [search](search.md) is enabled |
| `createPost(data: PostCreateInput!)` | create a draft |
| `updatePost(id, data: PostUpdateInput!)` | partial update of the draft; `null` clears a field |
| `deletePost(id)` | delete |
| `publishPost(id)` / `unpublishPost(id)` | copy the draft to `PUBLISHED` / remove the published version |
| `submitPostForReview`, `approvePost`, `requestChangesPost` | only when the [workflow](workflow.md) is enabled |

Every entry type also has these system fields:

| Field | |
|---|---|
| `id` | opaque, expiring id ([why](ids-and-refs.md)) |
| `ref` | stable opaque reference, for cache keys |
| `stage` | `DRAFT` or `PUBLISHED`, the version you're looking at |
| `createdAt`, `updatedAt`, `publishedAt` | timestamps |
| `workflowStatus` | `DRAFT`, `IN_REVIEW`, `CHANGES_REQUESTED`, `PUBLISHED` |
| `createdBy`, `updatedBy`, `history` | editorial metadata; `null` for anonymous readers |

Relation fields resolve to the related entries **in the same stage**: a
published post shows its published author. Related entries that are missing
or unpublished are skipped.

## Writing content

```graphql
mutation($data: PostCreateInput!) {
  createPost(data: $data) { id ref stage workflowStatus }
}
```

```json
{ "data": { "title": "Hello", "slug": "hello", "author": "<author id>", "tags": ["<tag id>"] } }
```

Relation inputs take entry `id`s, which must be fresh (they expire).
`updatePost` merges the given fields into the draft; send `null` to clear a
field. Required fields must always be present.

## Stages

Queries return `PUBLISHED` content by default. Ask for drafts with
`stage: DRAFT`; that requires being signed in. Publishing copies the current
draft to the published version.

## Schema changes on running instances

When an Admin changes a content model, the instance handling the request
rebuilds its schema immediately. Other instances do so within
`TESS_SCHEMA_REFRESH_SECS`. A request can't use the operations of a type
created in the same request.
