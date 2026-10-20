# PHP client

Plain PHP 8.1+ with `ext-curl` and `ext-json` (no Composer): list published posts, full-text search,
lookup by slug, `GraphQLException::$errorCode`, and with an admin token: create + publish a post and
upload a generated text file.

```sh
export TESS_URL=http://localhost:8080
php cms_client.php
# admin part: export TESS_ADMIN_TOKEN first
```

No local PHP? `docker run --rm --network host -v "$PWD":/app -w /app -e TESS_URL -e TESS_ADMIN_TOKEN php:8.3-cli-alpine php cms_client.php`
