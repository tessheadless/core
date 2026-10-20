---
sidebar_position: 2
title: Configuration reference
---

# Configuration reference

Tess is configured only through **environment variables**. Secrets must come
from the environment or a secrets manager, never from files committed to a
repository. A commented template is provided as `.env.example`.

Generate every secret with a fresh random value:

```bash
openssl rand -hex 32
```

## Secrets and authentication

| Variable | Default | Description |
|---|---|---|
| `TESS_ID_SECRET` | **required** | Key encrypting the opaque API ids. At least 32 bytes. |
| `TESS_REF_SECRET` | **required** | Key for stable `ref` values. At least 32 bytes, different from `TESS_ID_SECRET`. |
| `TESS_SESSION_SECRET` | **required** | Key signing user sessions. At least 32 bytes. Changing it signs everybody out. |
| `TESS_ADMIN_TOKEN` | unset | Bootstrap token with full access (acts as the "system" Owner). At least 32 characters. Use it to create the first Owner, then remove it. |
| `TESS_ID_SECRET_PREVIOUS` | unset | Comma-separated previous id secrets, still accepted for decoding during a [key rotation](security.md#rotating-the-id-secret). |
| `TESS_ID_TTL` | `24h` | Lifetime of opaque ids (`900s`, `30m`, `24h`, `7d`). |
| `TESS_SESSION_TTL` | `12h` | Lifetime of user sessions. |
| `TESS_SECRETS_KEY` | `TESS_SESSION_SECRET` | Key encrypting integration secrets stored in the database (webhook signing secrets and headers, storage keys of translation delivery targets). At least 32 bytes. Changing it makes them unreadable: re-enter them. |

## Database

| Variable | Default | Description |
|---|---|---|
| `TESS_DATABASE` | `memory` | `postgres`, `mysql`, `sqlite`, `mongodb`, `dynamodb`, `rocksdb` or `memory`. The Docker image sets `rocksdb`. |
| `DATABASE_URL` | | For `postgres`, `mysql`, `sqlite`: `postgres://user:pass@host:5432/db`, `mysql://user:pass@host:3306/db?ssl-mode=required`, `sqlite:///data/tess.db`. Its scheme must match `TESS_DATABASE`. |
| `TESS_DB_MAX_CONNECTIONS` | `10` (`2` on Lambda) | SQL connection pool size. |
| `MONGODB_URL` | | For `mongodb`. |
| `MONGODB_DATABASE` | `tess` | MongoDB database name. |
| `DYNAMODB_TABLE` | `tess` | DynamoDB table (key schema `pk`/`sk`). |
| `DYNAMODB_ENDPOINT` | unset | Custom endpoint, e.g. DynamoDB Local. |
| `DYNAMODB_CREATE_TABLE` | `false` | Create the table at startup if missing (local development). |
| `ROCKSDB_PATH` | `data/rocksdb` | RocksDB directory. |

AWS credentials and region for DynamoDB come from the standard AWS chain: the
IAM role of the function or task, `AWS_*` variables, or a profile. See
[Databases](databases.md).

## Media

| Variable | Default | Description |
|---|---|---|
| `TESS_MEDIA_STORE` | `fs` (`s3` on Lambda) | `fs` or `s3`. |
| `TESS_MEDIA_DIR` | `data/media` | Directory for `fs`. |
| `TESS_S3_BUCKET` | | Bucket for `s3` (required). |
| `TESS_S3_ENDPOINT` | unset | Endpoint of S3-compatible services (Garage, MinIO, R2…). |
| `TESS_S3_REGION` | from the AWS chain | Region (Garage uses `garage` by default). |
| `TESS_S3_FORCE_PATH_STYLE` | `true` when an endpoint is set | Path-style URLs, needed by most self-hosted services. |
| `TESS_S3_ACCESS_KEY_ID` / `TESS_S3_SECRET_ACCESS_KEY` | unset | Explicit credentials. Set both, or neither to use the default AWS chain. |
| `TESS_S3_CREATE_BUCKET` | `false` | Create the bucket at startup if missing. |
| `TESS_MEDIA_PUBLIC_URL` | unset | Public base URL (CDN) of the bucket: asset URLs become `{base}/{key}`. |
| `TESS_MEDIA_URL_TTL_SECS` | `3600` | Lifetime of presigned upload/download URLs. |
| `TESS_MEDIA_MAX_UPLOAD_BYTES` | `104857600` (100 MiB) | Maximum file size. |

See [Media storage](media-storage.md).

## Search

| Variable | Default | Description |
|---|---|---|
| `TESS_SEARCH` | `none` | `none`, `typesense` or `memory`. |
| `TYPESENSE_URL` | | e.g. `http://typesense:8108`. |
| `TYPESENSE_API_KEY` | | Typesense admin API key. |
| `TYPESENSE_COLLECTION_PREFIX` | `tess_` | Prefix of collection names, to share a cluster. |

See [Search](search.md).

## Cache

| Variable | Default | Description |
|---|---|---|
| `TESS_CACHE` | `none` | `none`, `valkey` (Valkey or any Redis-compatible server, shared by every instance) or `memory` (one instance). |
| `VALKEY_URL` | | `redis://[user:password@]host:6379/0`, or `rediss://` for TLS. |
| `TESS_CACHE_TTL_SECS` | `60` | Lifetime of cached responses; capped at half of `TESS_MEDIA_URL_TTL_SECS` and `TESS_ID_TTL`. |
| `TESS_CACHE_PREFIX` | `tess:` | Prefix of keys and channels, to share a server between installs. |
| `TESS_CACHE_MAX_ENTRIES` | `10000` | `memory` only. |

See [Cache](cache.md).

## Plugins

| Variable | Default | Description |
|---|---|---|
| `TESS_PLUGINS` | `true` | Enable the WebAssembly plugin system. |
| `TESS_PLUGIN_ALLOW_HTTP` | `true` | Whether admins may allow plugins to make HTTP requests at all. |
| `TESS_PLUGIN_MAX_TIMEOUT_MS` | `10000` | Highest per-plugin timeout an admin can set (plugins default to 1000 ms). |
| `TESS_PLUGIN_MAX_MEMORY_PAGES` | `1024` (64 MiB) | Highest per-plugin memory cap, in 64 KiB pages (plugins default to 256 = 16 MiB). |
| `TESS_PLUGIN_MAX_MODULE_BYTES` | `20971520` (20 MiB) | Maximum module size. |

## Translations, webhooks and workflows

| Variable | Default | Description |
|---|---|---|
| `TESS_I18N` | `true` | Enable [translations](../integrations/translations.md) (API, `/i18n/{format}/{file}`, delivery targets). |
| `TESS_WEBHOOKS` | `true` | Enable [webhooks](../integrations/webhooks.md). |
| `TESS_WEBHOOK_ALLOWED_HOSTS` | unset (any) | Comma-separated hosts webhooks may target (`*.example.com` allowed). |
| `TESS_WEBHOOK_TIMEOUT_SECS` | `10` | Timeout of one delivery attempt. |
| `TESS_WEBHOOK_MAX_ATTEMPTS` | `5` | Attempts per delivery (retries after 10 s, 1 min, 5 min, 30 min). On AWS Lambda: one inline attempt. |
| `TESS_N8N_URL` | unset | Where Tess reaches [n8n](../integrations/n8n.md), e.g. `http://n8n:5678`. Enables the `/n8n/` proxy and the admin *Workflows* page. Not on AWS Lambda. |
| `TESS_N8N_PATH` | `/n8n/` | Path the n8n editor is served under; n8n must use the same `N8N_PATH`. |

## HTTP and GraphQL

| Variable | Default | Description |
|---|---|---|
| `PORT` | `8080` | HTTP port (ignored on Lambda). |
| `TESS_PUBLIC_READ` | `true` | Anonymous requests may read `PUBLISHED` content. |
| `TESS_CORS_ORIGINS` | `*` | Comma-separated allowed origins, or `*`. |
| `TESS_GRAPHIQL` | `true` | Serve the GraphiQL IDE on `GET /graphql`. |
| `TESS_INTROSPECTION` | `true` | GraphQL introspection (the admin UI's review queue uses it). |
| `TESS_DEFAULT_PAGE_SIZE` | `100` | Default `first` of list queries. |
| `TESS_MAX_PAGE_SIZE` | `1000` | Maximum `first` of list queries. |
| `TESS_MAX_QUERY_DEPTH` | `16` | Maximum GraphQL query depth. |
| `TESS_SCHEMA_REFRESH_SECS` | `10` | How often an instance checks for model and plugin changes made elsewhere. |
| `TESS_ADMIN_DIR` | `admin/dist/client` if present | Directory of the built admin UI, served under `/admin/`. |

## Logging

| Variable | Default | Description |
|---|---|---|
| `RUST_LOG` | `info` | Log filter, e.g. `info,tess_graphql=debug`. |
| `TESS_LOG_FORMAT` | text (`json` on Lambda) | Set to `json` for structured logs. |

## Build-time features

The `tess-server` package has one cargo feature per backend, so you can build
smaller binaries:

| Feature | Enables |
|---|---|
| `sql` (aliases `postgres`, `mysql`, `sqlite`) | the SQL backend |
| `mongodb` | MongoDB |
| `dynamodb` | DynamoDB |
| `rocksdb` | RocksDB (needs `clang` and `cmake` to build) |
| `s3` | S3-compatible media store |
| `lambda` | AWS Lambda runtime |

All are on by default. For example:

```bash
cargo build --release -p tess-server --no-default-features --features sql,s3
# binary: target/release/tess
```

Choosing a backend that the binary was built without fails at startup with a
clear message.
