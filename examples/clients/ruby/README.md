# Ruby client

Stdlib only (`net/http`, `json`): list published posts, full-text search (`searchPosts`), lookup by slug,
GraphQL error codes, and with an admin token: create a draft post and publish it.

Prerequisites: Ruby 3.x.

```sh
export TESS_URL=http://localhost:8080
ruby cms_client.rb
# admin part: export TESS_ADMIN_TOKEN first (e.g. source your dev env file)
```
