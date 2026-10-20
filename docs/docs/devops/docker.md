---
sidebar_position: 3
title: Docker
---

# Docker and Docker Compose

## The image

The `Dockerfile` builds Tess in three stages: the admin UI (Node), the server
(Rust), and a slim Debian runtime image.

```bash
docker build -t tess .
# Smaller image with only some backends:
docker build --build-arg FEATURES=sql,s3 -t tess:sql .
```

| | |
|---|---|
| Binary | `/usr/local/bin/tess` |
| User | `tess` (uid 10001), non-root |
| Admin UI | `/app/admin`, served at `/admin/` |
| Volume | `/data` (RocksDB database and local media) |
| Port | `8080` |
| Defaults | `TESS_DATABASE=rocksdb`, `ROCKSDB_PATH=/data/rocksdb`, `TESS_MEDIA_STORE=fs`, `TESS_MEDIA_DIR=/data/media`, `TESS_LOG_FORMAT=json` |

The build argument `FEATURES` selects the [cargo features](configuration.md#build-time-features)
(default: `sql,mongodb,dynamodb,rocksdb,s3`).

## Local: Tess alone (`docker-compose-local.yml`)

For local use, `docker-compose-local.yml` runs only Tess: no database
server, no S3, no Typesense. Content is stored in an embedded **RocksDB**
database and uploaded files on disk, both in the `tess-local-data` volume.
Search is disabled.

```bash
cp .env.example .env   # fill TESS_ID_SECRET, TESS_REF_SECRET, TESS_SESSION_SECRET, TESS_ADMIN_TOKEN
docker compose -f docker-compose-local.yml up --build
```

Open `http://localhost:8080/admin/` and create the first Owner with
`TESS_ADMIN_TOKEN`; afterwards you can empty that variable. Data survives
restarts; `docker compose -f docker-compose-local.yml down -v` deletes it.
Compose stops with a clear message if a required secret is missing.

## Docker Compose

`docker-compose.yml` runs Tess alone by default. Optional services are
**profiles**:

| Profile | Service | Then set in `.env` |
|---|---|---|
| `postgres` | PostgreSQL 17 | `TESS_DATABASE=postgres`, `DATABASE_URL=postgres://tess:…@postgres:5432/tess`, `POSTGRES_PASSWORD` |
| `mysql` | MySQL 8.4 | `TESS_DATABASE=mysql`, `DATABASE_URL=mysql://tess:…@mysql:3306/tess?ssl-mode=required`, `MYSQL_PASSWORD` |
| `mongodb` | MongoDB 8 | `TESS_DATABASE=mongodb`, `MONGODB_URL=mongodb://mongo:27017` |
| `dynamodb` | DynamoDB Local | `TESS_DATABASE=dynamodb`, `DYNAMODB_ENDPOINT=http://dynamodb:8000`, `DYNAMODB_CREATE_TABLE=true`, dummy `AWS_*` |
| `s3` | Garage | `TESS_MEDIA_STORE=s3`, `TESS_S3_*`, `GARAGE_RPC_SECRET`, `GARAGE_ADMIN_TOKEN` |
| `search` | Typesense 29 | `TESS_SEARCH=typesense`, `TYPESENSE_URL=http://typesense:8108`, `TYPESENSE_API_KEY` |

### First run

```bash
cp .env.example .env
# Fill TESS_ADMIN_TOKEN, TESS_ID_SECRET, TESS_REF_SECRET, TESS_SESSION_SECRET
# (one `openssl rand -hex 32` each), plus the passwords of the profiles you use.
docker compose --profile postgres --profile search up -d --build
```

1. Open `http://localhost:8080/admin/` and create the first Owner with the
   bootstrap token.
2. Remove `TESS_ADMIN_TOKEN` from `.env`.
3. Run `docker compose up -d` again to apply it.

:::tip
Put Tess behind a reverse proxy that terminates TLS (Caddy, Traefik, nginx),
and set `TESS_CORS_ORIGINS` to your front-end origins. See
[Security](security.md).
:::

### Garage (S3-compatible storage)

Garage needs a one-time setup: a cluster layout, a bucket and an access key.
The image has no shell, so the repository provides a script that runs the
`garage` CLI through `docker exec`:

```bash
key_id="GK$(openssl rand -hex 12)"   # Garage key ids look like GK + 24 hex chars
key_secret="$(openssl rand -hex 32)"
deploy/garage/init.sh <garage-container-name> tess-media "$key_id" "$key_secret"
```

Then set `TESS_S3_BUCKET=tess-media`, `TESS_S3_ENDPOINT=http://garage:3900`,
`TESS_S3_REGION=garage`, `TESS_S3_ACCESS_KEY_ID=$key_id` and
`TESS_S3_SECRET_ACCESS_KEY=$key_secret`. The script is idempotent. Garage's
own configuration is `deploy/garage/garage.toml`; its secrets come from the
environment (`GARAGE_RPC_SECRET`, `GARAGE_ADMIN_TOKEN`).

## Without Docker

```bash
cargo build --release -p tess-server            # needs clang + cmake for RocksDB
(cd admin && npm ci && npm run build)            # optional: the admin UI
TESS_ADMIN_DIR=admin/dist/client ./target/release/tess
```

For a quick throwaway instance (in-memory database, random secrets), use
`scripts/dev-server.sh`.
