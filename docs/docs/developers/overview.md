---
sidebar_position: 1
title: Overview
---

# Building with the Tess API

Tess exposes one **GraphQL** endpoint, generated from your content models.

| | |
|---|---|
| Endpoint | `POST /graphql`, with JSON `{ "query": "…", "variables": { … } }` |
| IDE | `GET /graphql` opens GraphiQL, if enabled by the operator |
| Schema | `GET /graphql/schema.graphql` (SDL) |
| Auth | `Authorization: Bearer <token>` (session, API token); none for public reads |

```bash
curl -s https://cms.example.com/graphql \
  -H 'content-type: application/json' \
  -d '{"query":"{ posts(first: 3) { total items { title } } }"}'
```

```ts
const res = await fetch('https://cms.example.com/graphql', {
  method: 'POST',
  headers: {'content-type': 'application/json'},
  body: JSON.stringify({query: '{ posts(first: 3) { items { ref title } } }'}),
});
const {data, errors} = await res.json();
```

## What you can do

- [Model content and use the generated API](graphql-api.md)
- [Filter, sort and paginate](querying.md)
- [Authenticate users and machines](authentication.md)
- [Use the review workflow](workflow.md)
- [Upload and serve media](media.md)
- [Search](search.md)
- [Handle ids and refs correctly](ids-and-refs.md)
- [Handle errors](errors.md)
- [Start from a client example](clients.md) in Vike, Gatsby, Next.js, Nuxt,
  Ruby, Python, Lua, PHP, Go or Rust
