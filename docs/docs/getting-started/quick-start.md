---
sidebar_position: 1
title: Quick start
---

# Quick start

Run Tess locally, create the first account, model some content and query it,
in about five minutes.

## 1. Start Tess

### With Docker Compose (recommended)

```bash
cp .env.example .env
# Fill in the four secrets, each with a different random value:
#   TESS_ADMIN_TOKEN, TESS_ID_SECRET, TESS_REF_SECRET, TESS_SESSION_SECRET
openssl rand -hex 32
docker compose up --build
```

To run every optional service too (S3 storage, search, cache and n8n
workflows, all on your machine), use the `local` profile:
`scripts/init-env.sh --local` then `docker compose --profile local up --build`.

By default Tess stores content in an embedded RocksDB database and keeps
uploaded files on disk, both in the `tess-data` volume. To use PostgreSQL,
MySQL, MongoDB, DynamoDB, an S3 bucket or Typesense instead, see
[Docker](../devops/docker.md).

### From source

```bash
scripts/dev-server.sh   # in-memory database, random secrets, port 8080
```

The script writes its generated secrets to a private file
(`${XDG_RUNTIME_DIR:-/tmp}/tess-dev.env`) and never prints them.

## 2. Create the first Owner

Until the first account exists, `setupRequired` is `true`. Open
[http://localhost:8080/admin/](http://localhost:8080/admin/) and follow the **Create the first Owner**
screen, which asks for the bootstrap token (`TESS_ADMIN_TOKEN`).

You can also do it from a terminal:

```bash
source "${XDG_RUNTIME_DIR:-/tmp}/tess-dev.env"   # or export TESS_ADMIN_TOKEN yourself
curl -s http://localhost:8080/graphql \
  -H 'content-type: application/json' \
  -H "authorization: Bearer $TESS_ADMIN_TOKEN" \
  -d '{"query":"mutation { createUser(input: {email: \"ada@example.com\", name: \"Ada Example\", role: OWNER, password: \"choose a long passphrase\"}) { id role } }"}'
```

:::warning
The bootstrap token grants full access. Once the Owner exists, remove
`TESS_ADMIN_TOKEN` from your configuration and restart.
:::

## 3. Sign in

```graphql
mutation {
  login(email: "ada@example.com", password: "choose a long passphrase") {
    token
    expiresAt
  }
}
```

Send the returned token as `Authorization: Bearer <token>` with every request
below. In the admin UI, just sign in with your email and password.

## 4. Model content

```graphql
mutation {
  createContentType(input: {
    apiId: "Post"
    fields: [
      { name: "title", type: TEXT, required: true }
      { name: "slug", type: TEXT }
      { name: "body", type: MARKDOWN }
    ]
  }) { apiId pluralApiId }
}
```

Tess immediately adds `post`, `posts`, `createPost`, `updatePost`,
`publishPost` and friends to the schema.

## 5. Create, publish, query

```graphql
mutation {
  createPost(data: { title: "Hello Tess", slug: "hello-tess", body: "First tile." }) { id }
}
```

```graphql
mutation($id: ID!) {
  publishPost(id: $id) { stage title }
}
```

Anyone can now read it without a token, and the GraphiQL IDE at
[http://localhost:8080/graphql](http://localhost:8080/graphql) is a handy place to try:

```graphql
{
  posts(where: { slug: "hello-tess" }) {
    total
    items { ref title body publishedAt }
  }
}
```

## Next steps

- [Concepts](concepts.md): stages, review workflow, roles, ids and refs.
- [Admin UI](admin-ui.md): a tour of the admin.
- [Build with the API](../developers/overview.md): filters, auth, media, search.
- [Deploy and operate](../devops/overview.md): production setup.
