---
sidebar_position: 2
title: Concepts
---

# Concepts

## Content types and entries

A **content type** (e.g. `Post`, `Author`, `Page`) is a model with typed
fields: `TEXT` (with length bounds and an optional format), `MARKDOWN`,
`INT`, `FLOAT`, `BOOLEAN`, `DATETIME`, `DATE`, `UUID`, `PASSWORD`
(write-only), `JSON`, `ENUM`, `RELATION` to another type (one or many),
`COMPONENT` and `DYNAMIC_ZONE`. Content types are created at runtime, and
the GraphQL schema is rebuilt from them. See
[the GraphQL API](../developers/graphql-api.md).

**Components** are reusable groups of fields (an SEO block, a hero banner)
that you add to content types, nest in each other, or mix in **dynamic
zones** to build pages from blocks. See
[Components](../developers/components.md).

An **entry** is one item of a content type, such as a single blog post.

## Stages: draft and published

Every entry has a **draft**, which is what editors change, and optionally a
**published** version, which is what the public reads. Publishing copies the
draft to the published stage. Editing afterwards changes only the draft until
the next publish. Queries take a `stage` argument (`PUBLISHED` by default).

## Review workflow

Admins can enable a **review workflow** on any content type, typically pages
and blog posts. On those types, contributors submit drafts for review, and
reviewers approve them (which publishes them) or request changes with a
comment:

```text
DRAFT ──submit──▶ IN_REVIEW ──approve──▶ PUBLISHED
  ▲                   │
  └─ CHANGES_REQUESTED ◀── request changes (with a comment)
```

Editing an entry sets it back to `DRAFT`, so reviewers always approve exactly
what gets published. Every step is recorded in the entry's `history`. See
[Workflow](../developers/workflow.md).

## Users, roles and scope

| | Owner | Admin | Contributor | Reviewer | Member |
|---|:-:|:-:|:-:|:-:|:-:|
| Manage Owners | ✔ | | | | |
| Manage users, API tokens, content models, plugins | ✔ | ✔ | | | |
| Publish anything, bypass review | ✔ | ✔ | | | |
| Approve / request changes (workflow types) | ✔ | ✔ | | ✔ | |
| Create and edit drafts, submit for review, upload media | ✔ | ✔ | ✔ | | |
| Publish types **without** workflow | ✔ | ✔ | ✔ | | |
| Read drafts and media | ✔ | ✔ | ✔ | ✔ | ✔ |

- **Scope:** each Contributor or Reviewer can be limited to a list of
  content types, their **scope**. An empty scope means all.
- **API tokens** carry a role and a scope too.
- **Anonymous requests** can read published content, unless the operator
  disables public reads.
- **Where the rules live:** they are enforced in the core, so every surface
  (API, uploads, plugins) follows them.

See [Authentication](../developers/authentication.md).

## Ids and refs

The `id` of an entry is an **opaque, encrypted token that expires** (24 h by
default). It changes on every response, and database ids are never exposed.
Use the stable **`ref`** as a cache or list key, and route pages by your own
fields, such as a `slug`. See [Ids and refs](../developers/ids-and-refs.md).

## Media, search and plugins

- **Media**: uploaded files are entries of the built-in `Asset` type. Any
  content type can reference them with a relation. See
  [Media](../developers/media.md).
- **Search**: with Typesense configured, every content type with text fields
  gets a `search<Plural>` query. See [Search](../developers/search.md).
- **Plugins**: sandboxed WebAssembly modules that validate, transform or
  react to changes. See [Plugins](../plugins/overview.md).
