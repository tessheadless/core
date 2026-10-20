---
sidebar_position: 2.5
title: Components and dynamic zones
---

# Components and dynamic zones

A **component** is a reusable set of fields: an SEO block, a
call-to-action, a hero banner, an address. You define it once and use it
in three ways:

| Use | Field | Stored as |
|---|---|---|
| One instance in a content type | `COMPONENT` | an object |
| A list of instances | `COMPONENT` with `repeatable: true` | a list of objects |
| A list mixing several components (page builder) | `DYNAMIC_ZONE` with `components: [...]` | a list of objects, each tagged with its component |
| Inside another component | `COMPONENT` in the component | nested objects |

Component data lives **inside the entry**, so it is versioned with the
draft/published stages and works on every database.

## Defining components

Admins manage components on the **Components** page of the admin UI, or
with the API:

```graphql
mutation {
  link: createComponent(input: {apiId: "Link", fields: [
    { name: "label", type: TEXT, required: true, maxLength: 40 }
    { name: "url",   type: TEXT, pattern: "https?://\\S+" }
    { name: "page",  type: RELATION, relationTarget: "Page" }
  ]}) { apiId }
  hero: createComponent(input: {apiId: "Hero", description: "Top banner", fields: [
    { name: "title", type: TEXT, required: true }
    { name: "image", type: RELATION, relationTarget: "Asset" }
    { name: "cta",   type: COMPONENT, component: "Link" }
  ]}) { apiId }
}
```

- **Shared names:** API IDs are PascalCase and shared with content types,
  so a component can't be named like a content type.
- **Allowed fields:** components can contain every field type except
  `PASSWORD` and `DYNAMIC_ZONE`.
- **Nesting:** components can be nested up to 5 levels deep, and never
  inside themselves.
- **Editing:** `updateComponent` replaces the definition, and every model
  using the component changes immediately.
- **Deleting:** `deleteComponent` is refused while a content type or
  component still uses it.
- **Listing:** `components` and `component(apiId)` return the definitions.

## Using them in content types

```graphql
mutation {
  createContentType(input: {apiId: "Page", fields: [
    { name: "title",  type: TEXT, required: true }
    { name: "links",  type: COMPONENT, component: "Link", repeatable: true }
    { name: "blocks", type: DYNAMIC_ZONE, components: ["Hero", "Link"] }
  ]}) { apiId }
}
```

## Reading

A component field returns the component's object type. A dynamic zone
returns a **union**, so select each possible component with an inline
fragment:

```graphql
{
  pages(where: { title: "Home" }) {
    items {
      title
      links { label url }
      blocks {
        __typename
        ... on Hero { title cta { label url page { title } } image { url } }
        ... on Link { label url }
      }
    }
  }
}
```

Relations inside components resolve in the same stage as the entry: a
published page shows published related entries.

## Writing

Component fields take an input object named after the component
(`LinkInput`). GraphQL has no input unions, so each **dynamic zone item**
is an object with **exactly one** key, the component to use:

```graphql
mutation($data: PageCreateInput!) { createPage(data: $data) { id } }
```

```json
{
  "data": {
    "title": "Home",
    "links": [{ "label": "Docs", "url": "https://tessheadless.app/doc/" }],
    "blocks": [
      { "Hero": { "title": "Welcome", "cta": { "label": "Start", "url": "https://example.com" } } },
      { "Link": { "label": "More" } }
    ]
  }
}
```

- **Validation:** component fields are validated like top-level fields,
  and errors name the exact path (`blocks[0].cta.url does not match the
  expected format`).
- **Relation ids:** relation ids inside components are the usual opaque
  `id`s.

:::note
Filtering and sorting by fields inside components isn't supported yet.
:::
