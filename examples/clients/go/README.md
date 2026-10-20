# Go client

Stdlib-only module with a small typed client (`Client.Do`, `Client.Upload`): list published posts,
full-text search, lookup by slug, `*GraphQLError` with `extensions.code` (use `errors.As`), and with an
admin token: create + publish a post and upload a generated text file.

Prerequisites: Go 1.22+.

```sh
export TESS_URL=http://localhost:8080
go run .
# admin part: export TESS_ADMIN_TOKEN first
go vet ./...
```
