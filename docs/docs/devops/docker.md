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

## Docker Compose

`docker-compose.yml` runs **Tess alone** by default: content in an embedded
RocksDB database and uploaded files on disk, both in the `tess-data` volume.
Optional services are **profiles**, and `scripts/init-env.sh` writes a `.env`
for the ones you pick.

```bash
scripts/init-env.sh -y            # Tess alone
docker compose up -d --build
```

### Everything on one machine: the `local` profile

The `local` profile adds every service Tess can use, self-hosted next to it:
**Garage** (S3 media), **Typesense** (search), **Valkey** (cache) and **n8n**
(workflows). `init-env.sh --local` writes the matching `.env`, secrets
included:

```bash
scripts/init-env.sh --local
docker compose --profile local up -d --build
```

Garage is set up automatically: the `garage-init` service creates its layout,
the `TESS_S3_BUCKET` bucket and the `TESS_S3_ACCESS_KEY_ID` key at every start
(it's idempotent), and Tess starts once it's done. Media URLs use
`TESS_S3_PUBLIC_ENDPOINT` (`http://localhost:3900`), the Garage port published
for your browser.

Add a database profile if you want one, e.g. `--profile local --profile postgres`.

With the commercial license, which doesn't include the embedded n8n editor,
use `local-commercial` instead: the same services without n8n.

```bash
scripts/init-env.sh --local --license commercial   # then fill TESS_LICENSE_KEY in .env
docker compose --profile local-commercial up -d --build
```

### Profiles

| Profile | Service | Then set in `.env` |
|---|---|---|
| `local` | Garage, Typesense, Valkey and n8n | all of the `s3`, `search`, `cache` and `n8n` settings below (`init-env.sh --local`) |
| `local-commercial` | Garage, Typesense and Valkey (no n8n) | the same without n8n, with `TESS_LICENSE=commercial` and `TESS_LICENSE_KEY` (`init-env.sh --local --license commercial`) |
| `postgres` | PostgreSQL 17 | `TESS_DATABASE=postgres`, `DATABASE_URL=postgres://tess:…@postgres:5432/tess`, `POSTGRES_PASSWORD` |
| `mysql` | MySQL 8.4 | `TESS_DATABASE=mysql`, `DATABASE_URL=mysql://tess:…@mysql:3306/tess?ssl-mode=required`, `MYSQL_PASSWORD` |
| `mongodb` | MongoDB 8 | `TESS_DATABASE=mongodb`, `MONGODB_URL=mongodb://mongo:27017` |
| `dynamodb` | DynamoDB Local | `TESS_DATABASE=dynamodb`, `DYNAMODB_ENDPOINT=http://dynamodb:8000`, `DYNAMODB_CREATE_TABLE=true`, dummy `AWS_*` |
| `s3` | Garage | `TESS_MEDIA_STORE=s3`, `TESS_S3_*` (with `TESS_S3_PUBLIC_ENDPOINT=http://localhost:3900`), `GARAGE_RPC_SECRET`, `GARAGE_ADMIN_TOKEN` |
| `search` | Typesense 29 | `TESS_SEARCH=typesense`, `TYPESENSE_URL=http://typesense:8108`, `TYPESENSE_API_KEY` |
| `cache` (or `cache-redis`, `cache-keydb`, `cache-dragonfly`) | Valkey (or Redis, KeyDB, Dragonfly) | `TESS_CACHE=valkey`, `TESS_CACHE_URL=redis://valkey:6379`, see [Cache](cache.md) |
| `n8n` | n8n with the Tess nodes | `TESS_N8N_URL=http://n8n:5678`, `N8N_ENCRYPTION_KEY`, see [n8n](../integrations/n8n.md) |
| `caldiy` | cal.diy, and PostgreSQL shared with Tess | `TESS_DATABASE=postgres` and the `CALDIY_*` settings (`init-env.sh --calendar caldiy`), see [Calendars](../integrations/calendars.md#) |

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
With the `s3` and `local` profiles, the `garage-init` service does it through
Garage's admin API (`deploy/garage/setup.sh`) from these `.env` settings:

```bash
TESS_S3_BUCKET=tess-media
TESS_S3_ENDPOINT=http://garage:3900            # how Tess reaches Garage
TESS_S3_PUBLIC_ENDPOINT=http://localhost:3900  # how browsers reach it (file URLs)
TESS_S3_REGION=garage
TESS_S3_ACCESS_KEY_ID=GK…                      # GK + 24 hex: "GK$(openssl rand -hex 12)"
TESS_S3_SECRET_ACCESS_KEY=…                    # 64 hex: openssl rand -hex 32
GARAGE_RPC_SECRET=…                            # openssl rand -hex 32
GARAGE_ADMIN_TOKEN=…                           # openssl rand -hex 32
```

`init-env.sh` generates them. Garage's own configuration is
`deploy/garage/garage.toml`. For a Garage outside Compose,
`deploy/garage/init.sh <container> <bucket> <key-id> <key-secret>` does the
same through `docker exec`.

## Without Docker

```bash
cargo build --release -p tess-server            # needs clang + cmake for RocksDB
(cd admin && npm ci && npm run build)            # optional: the admin UI
TESS_ADMIN_DIR=admin/dist/client ./target/release/tess
```

For a quick throwaway instance (in-memory database, random secrets), use
`scripts/dev-server.sh`.
