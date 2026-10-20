#!/usr/bin/env bash
# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

# Creates the demo content model used by every example in examples/clients:
#   Author { name }  and  Post { title, slug, excerpt, body, tags (JSON), author -> Author }
# plus a few published posts. Requires a running server and the admin token:
#   source "${XDG_RUNTIME_DIR:-/tmp}/tess-dev.env"; TESS_URL=http://localhost:8080 scripts/seed.sh
set -euo pipefail
URL="${TESS_URL:-http://localhost:8080}/graphql"
: "${TESS_ADMIN_TOKEN:?export TESS_ADMIN_TOKEN first}"
gql() {
  curl -sS --fail-with-body "$URL" -H 'content-type: application/json' \
    -H "authorization: Bearer $TESS_ADMIN_TOKEN" -d "$1"
}
id_of() { python3 -c 'import json,sys; d=json.load(sys.stdin); assert "errors" not in d, d; print(d["data"][sys.argv[1]]["id"])' "$1"; }

gql '{"query":"mutation { createContentType(input: {apiId: \"Author\", fields: [{name: \"name\", type: TEXT, required: true}]}) { apiId } }"}' >/dev/null
gql '{"query":"mutation { createContentType(input: {apiId: \"Post\", fields: [{name: \"title\", type: TEXT, required: true}, {name: \"slug\", type: TEXT, pattern: \"[a-z0-9]+(?:-[a-z0-9]+)*\"}, {name: \"excerpt\", type: TEXT, maxLength: 200}, {name: \"body\", type: MARKDOWN}, {name: \"tags\", type: JSON}, {name: \"author\", type: RELATION, relationTarget: \"Author\"}]}) { apiId } }"}' >/dev/null

author=$(gql '{"query":"mutation { createAuthor(data: {name: \"Ada Example\"}) { id } }"}' | id_of createAuthor)
gql "{\"query\":\"mutation(\$id: ID!) { publishAuthor(id: \$id) { id } }\",\"variables\":{\"id\":\"$author\"}}" >/dev/null
while IFS='|' read -r title slug excerpt; do
  vars=$(python3 -c 'import json,sys; print(json.dumps({"d": {"title": sys.argv[1], "slug": sys.argv[2], "excerpt": sys.argv[3], "body": sys.argv[3] + " (full text)", "tags": ["demo"], "author": sys.argv[4]}}))' "$title" "$slug" "$excerpt" "$author")
  post=$(gql "{\"query\":\"mutation(\$d: PostCreateInput!) { createPost(data: \$d) { id } }\",\"variables\":$vars}" | id_of createPost)
  gql "{\"query\":\"mutation(\$id: ID!) { publishPost(id: \$id) { id } }\",\"variables\":{\"id\":\"$post\"}}" >/dev/null
done <<'POSTS'
Hello headless world|hello-headless-world|Why a GraphQL-first CMS written in Rust.
Running on AWS Lambda|running-on-aws-lambda|The same binary, serverless, with DynamoDB and S3.
Writing a plugin|writing-a-plugin|Extend the CMS with sandboxed WebAssembly.
POSTS
echo "seeded: Author, Post (3 published posts)"
