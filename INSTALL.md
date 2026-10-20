# Installing Tess with Docker

Every [release](https://github.com/tessheadless/core/releases) ships Docker
images and a ready-to-use Compose bundle.

| Image | Tags |
|---|---|
| `ghcr.io/tessheadless/tess` | `<version>-amd64`, `<version>-arm64`, `<version>` (multi-arch), `latest…` |
| `ghcr.io/tessheadless/tess-n8n` | same tags: n8n with the Tess nodes, for the optional workflows |

## Requirements

- Docker 24+ with the Compose plugin (`docker compose version`).
- A Linux `amd64` (x86-64) or `arm64` (aarch64, e.g. Raspberry Pi 4/5 with a
  64-bit OS, AWS Graviton, Apple Silicon through Docker Desktop) host.

## 1. Download the bundle

Pick the bundle of your architecture (`uname -m`: `x86_64` → `amd64`,
`aarch64`/`arm64` → `arm64`):

```sh
VERSION=0.1.0     # the release you want
ARCH=amd64        # or arm64
curl -LO https://github.com/tessheadless/core/releases/download/v$VERSION/tess-docker-$VERSION-$ARCH.tar.xz
tar -xJf tess-docker-$VERSION-$ARCH.tar.xz
cd tess-docker-$VERSION-$ARCH
```

It contains:

| File | |
|---|---|
| `docker-compose.yml` | Tess alone by default (embedded database, files on disk), plus optional services enabled with profiles: `local` for all of them on this machine, or PostgreSQL, MySQL, MongoDB, DynamoDB Local, Garage S3, Typesense, a cache server (Valkey, Redis, KeyDB or Dragonfly) and n8n one by one. |
| `.env` | Your configuration. It already pins the image: `TESS_VERSION` and `TESS_ARCH` (see below). |
| `.env.example` | The documented template `.env` was made from. |
| `scripts/init-env.sh` | Fills `.env` with secrets and the settings of the services you choose. |
| `deploy/garage/` | Garage (S3) configuration and setup, for the `s3` and `local` profiles. |

The image used is `${TESS_IMAGE}:${TESS_VERSION}-${TESS_ARCH}`, for example
`ghcr.io/tessheadless/tess:0.1.0-arm64`. Set `TESS_ARCH=` (empty) to use the
multi-arch tag instead, which Docker resolves for your host.

## 2. Fill in the secrets

`.env` needs random secrets. `scripts/init-env.sh` writes them, with the
settings of the services you pick, and keeps the pinned image:

```sh
scripts/init-env.sh -f              # asks what to run (Tess alone by default)
scripts/init-env.sh -f -y --local   # or: every optional service (the `local` profile)
scripts/init-env.sh -f -y --local --license commercial   # commercial use: no n8n, needs your key
```

Commercial use needs the commercial license: set `TESS_LICENSE=commercial` and
your `TESS_LICENSE_KEY` (`tess_…`) in `.env`. It doesn't include the embedded
n8n workflow editor.

Or, for Tess alone, generate the secrets of the bundled `.env` by hand:

```sh
for key in TESS_ADMIN_TOKEN TESS_SESSION_SECRET TESS_ID_SECRET TESS_REF_SECRET TESS_SECRETS_KEY; do
  sed -i.bak "s|^$key=.*|$key=$(openssl rand -hex 32)|" .env
done
rm -f .env.bak
chmod 600 .env
```

Keep `.env` private and back it up: losing `TESS_SECRETS_KEY` makes the stored
webhook and storage credentials unreadable, and changing `TESS_SESSION_SECRET`
signs everybody out.

## 3. Start Tess

The simplest setup (embedded RocksDB, files on disk, nothing else to run):

```sh
docker compose up -d
```

Everything on this machine, with the `.env` from `init-env.sh --local`:
Garage for media, Typesense for search, Valkey as cache and n8n for workflows.
Garage's bucket and key are created automatically at start:

```sh
docker compose --profile local up -d
```

Or pick services one by one, here PostgreSQL and full-text search (set the
variables each profile asks for in `.env`, e.g. `TESS_DATABASE=postgres`,
`DATABASE_URL`, `POSTGRES_PASSWORD`, `TESS_SEARCH=typesense`, `TYPESENSE_URL`,
`TYPESENSE_API_KEY`):

```sh
docker compose --profile postgres --profile search up -d
```

| Profile | Service | Set in `.env` |
|---|---|---|
| `local` | Garage, Typesense, Valkey and n8n | the settings of `s3`, `search`, `cache` and `n8n` below (`init-env.sh --local`) |
| `local-commercial` | Garage, Typesense and Valkey | the same without n8n, for the commercial license (`init-env.sh --local --license commercial`) |
| `postgres` | PostgreSQL 17 | `TESS_DATABASE=postgres`, `DATABASE_URL`, `POSTGRES_PASSWORD` |
| `mysql` | MySQL 8.4 | `TESS_DATABASE=mysql`, `DATABASE_URL`, `MYSQL_PASSWORD` |
| `mongodb` | MongoDB 8 | `TESS_DATABASE=mongodb`, `MONGODB_URL` |
| `dynamodb` | DynamoDB Local | `TESS_DATABASE=dynamodb`, `DYNAMODB_*`, `AWS_*` |
| `s3` | Garage | `TESS_MEDIA_STORE=s3`, `TESS_S3_*` (`TESS_S3_PUBLIC_ENDPOINT` as browsers reach Garage), `GARAGE_*` |
| `search` | Typesense | `TESS_SEARCH=typesense`, `TYPESENSE_URL`, `TYPESENSE_API_KEY` |
| `cache` | Valkey | `TESS_CACHE=valkey`, `TESS_CACHE_URL=redis://valkey:6379` |
| `cache-redis`, `cache-keydb`, `cache-dragonfly` | Redis, KeyDB or Dragonfly instead | `TESS_CACHE=redis` (or `keydb`, `dragonfly`), `TESS_CACHE_URL=redis://redis:6379` (or `keydb`, `dragonfly`) |
| `n8n` | n8n workflows | `TESS_N8N_URL=http://n8n:5678`, `N8N_ENCRYPTION_KEY` (see below) |

Check it runs:

```sh
curl http://localhost:8080/health      # {"status":"ok",...}
docker compose logs -f tess
```

## 4. Create the first account

1. Open <http://localhost:8080/admin/> and sign in with `TESS_ADMIN_TOKEN`.
2. Create the **Owner** account.
3. Empty `TESS_ADMIN_TOKEN` in `.env` and apply it:
   `docker compose up -d` (with the same `--profile` options).

The GraphQL API is at <http://localhost:8080/graphql> (GraphiQL in a browser).

## Workflows (n8n), optional

In `.env`:

```sh
TESS_N8N_URL=http://n8n:5678
N8N_ENCRYPTION_KEY=...                         # openssl rand -hex 32
N8N_PUBLIC_URL=http://localhost:8080/n8n/      # behind HTTPS: https://cms.example.com/n8n/
# N8N_SECURE_COOKIE=true                       # behind HTTPS
```

```sh
docker compose --profile n8n up -d     # also part of the `local` profile
```

If `N8N_ENCRYPTION_KEY` is empty, n8n generates a key and keeps it in its
volume. Set it anyway so the key is in your `.env` backup. Then open **Admin → Workflows**. See the
[n8n guide](https://tessheadless.com/docs/integrations/n8n).

## Upgrading

Set the new version in `.env`, then pull and restart:

```sh
sed -i.bak "s|^TESS_VERSION=.*|TESS_VERSION=0.2.0|" .env && rm -f .env.bak
docker compose pull       # with the same --profile options as `up`
docker compose up -d
```

Read the release notes first; database migrations run automatically at start.
Download the new bundle too when the release notes say the Compose files
changed (keep your `.env`).

## Backups

Data lives in Docker volumes: Tess's `/data` (database and files), plus the
volumes of the services you run (`docker volume ls`). For example, Tess's own:

```sh
docker compose stop tess
docker run --rm --volumes-from "$(docker compose ps -aq tess)" -v "$PWD":/backup alpine \
  tar -czf /backup/tess-data-$(date +%F).tar.gz -C /data .
docker compose start tess
```

Content models and users can also be exported with `tess-transfer` (see the
[import & export guide](https://tessheadless.com/docs/devops/import-export)).

## Production checklist

- Put Tess behind a TLS reverse proxy (Caddy, Traefik, nginx) and restrict
  `TESS_CORS_ORIGINS`.
- Keep `TESS_ADMIN_TOKEN` empty once the Owner exists.
- Back up the volumes and `.env`.
- More: [configuration reference](https://tessheadless.com/docs/devops/configuration),
  [security](https://tessheadless.com/docs/devops/security).

## Without Docker

Each release also has native archives (`tess-<version>-<os>-<arch>`) for Linux
(amd64, arm64, armv7), macOS (amd64, arm64) and Windows (amd64, arm64). They
hold `tess`, `tess-transfer` and the admin UI (`admin/`, found next to the
binary). Configure them with the same environment variables:

```sh
tar -xJf tess-0.1.0-linux-arm64.tar.xz && cd tess-0.1.0-linux-arm64
cp .env.example .env    # fill in the secrets, then:
set -a && . ./.env && set +a && ./tess
```

Release files are listed in `SHA256SUMS`: check them with `sha256sum -c SHA256SUMS --ignore-missing`.
