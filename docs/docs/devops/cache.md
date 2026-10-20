---
sidebar_position: 6.5
title: Cache
---

# Cache

Tess can cache public API responses in a server speaking the **Redis
protocol**: [Valkey](https://valkey.io), [Redis](https://redis.io),
[KeyDB](https://docs.keydb.dev), [Dragonfly](https://www.dragonflydb.io), or a
managed service. Use the one your team already runs. The cache is optional:
with `TESS_CACHE=none` (the default) nothing changes.

```bash
TESS_CACHE=valkey                     # valkey, redis, keydb or dragonfly
TESS_CACHE_URL=redis://valkey:6379    # rediss:// for TLS; redis://user:password@host:6379/0
TESS_CACHE_TTL_SECS=60
TESS_CACHE_PREFIX=tess:               # to share one server between installs
```

## Choosing a server

They all work the same way for Tess: it only uses `GET`, `SET … PX`, `INCR`,
`PUBLISH` and `SUBSCRIBE`. `TESS_CACHE` names the server in the logs and in
`/health`.

| Server | `TESS_CACHE` | Compose profile | License | Notes |
|---|---|---|---|---|
| Valkey 8 | `valkey` | `cache` | BSD 3-Clause | Linux Foundation fork of Redis 7.2. The default here. |
| Redis 8 | `redis` | `cache-redis` | RSALv2, SSPLv1 or AGPLv3 | Redis 6 and 7 work too. |
| KeyDB 6.3 | `keydb` | `cache-keydb` | BSD 3-Clause | Multithreaded fork of Redis. Its last release is from 2023. |
| Dragonfly 2 | `dragonfly` | `cache-dragonfly` | BSL 1.1 | Multithreaded, Redis-compatible; needs 256 MiB per thread. |
| Managed service | the engine's name | | | See [below](#managed-services). |

Licenses are given for information: check them with each vendor. All four are
tested with Tess (`TESS_TEST_CACHE_URL=… cargo test -p tess-cache`).

Not supported: **cluster mode** (Redis Cluster, ElastiCache "cluster mode
enabled") and **Sentinel**. Use a single primary endpoint: a cache needs no
sharding at this size, and a replica failover only empties it.

## Examples

### Docker Compose

`docker-compose.yml` has a service for each server, in its own profile, with no
persistence, a memory limit (`CACHE_MAXMEMORY`, default `256mb`) and LRU
eviction. Pick one:

```bash
# Valkey
TESS_CACHE=valkey     TESS_CACHE_URL=redis://valkey:6379       # docker compose --profile cache up -d
# Redis
TESS_CACHE=redis      TESS_CACHE_URL=redis://redis:6379        # docker compose --profile cache-redis up -d
# KeyDB
TESS_CACHE=keydb      TESS_CACHE_URL=redis://keydb:6379        # docker compose --profile cache-keydb up -d
# Dragonfly
TESS_CACHE=dragonfly  TESS_CACHE_URL=redis://dragonfly:6379    # docker compose --profile cache-dragonfly up -d
```

`scripts/init-env.sh --cache redis` (or `valkey`, `keydb`, `dragonfly`) writes
these settings and prints the matching command.

### Your own server

The services above, as `docker run` commands (any recent version works):

```bash
docker run -d --name valkey    -p 6379:6379 valkey/valkey:8-alpine \
  valkey-server --save "" --appendonly no --maxmemory 256mb --maxmemory-policy allkeys-lru

docker run -d --name redis     -p 6379:6379 redis:8-alpine \
  redis-server --save "" --appendonly no --maxmemory 256mb --maxmemory-policy allkeys-lru

docker run -d --name keydb     -p 6379:6379 eqalpha/keydb:latest \
  keydb-server --save "" --appendonly no --maxmemory 256mb --maxmemory-policy allkeys-lru

docker run -d --name dragonfly -p 6379:6379 --ulimit memlock=-1 docker.dragonflydb.io/dragonflydb/dragonfly:v2.0.2 \
  --cache_mode=true --maxmemory=256mb --proactor_threads=1 --dbfilename=
```

With a password (`requirepass`, or an ACL user), put it in the URL:
`redis://:password@host:6379`, or `redis://user:password@host:6379`. A
database number goes at the end: `redis://host:6379/2`.

### Managed services

| Service | `TESS_CACHE` | `TESS_CACHE_URL` |
|---|---|---|
| Amazon ElastiCache (Valkey or Redis OSS, cluster mode disabled) | `valkey` / `redis` | `rediss://:AUTH_TOKEN@master.my-cache.xxxxxx.euw1.cache.amazonaws.com:6379` (primary endpoint, with in-transit encryption) |
| Amazon MemoryDB | `valkey` / `redis` | Not supported: MemoryDB always runs in cluster mode. |
| Azure Cache for Redis | `redis` | `rediss://:ACCESS_KEY@my-cache.redis.cache.windows.net:6380` |
| Google Cloud Memorystore (Valkey or Redis) | `valkey` / `redis` | `redis://10.0.0.3:6379` (private IP; `rediss://` with in-transit encryption) |
| Upstash | `redis` | `rediss://default:PASSWORD@my-db.upstash.io:6379` |
| Redis Cloud | `redis` | `rediss://default:PASSWORD@redis-12345.c1.eu-west-1-2.ec2.redns.redis-cloud.com:12345` |

TLS (`rediss://`) checks the server certificate against the public CA roots.

## What is cached

| | Cached | Key |
|---|---|---|
| GraphQL **queries by anonymous visitors** (public reads, `PUBLISHED` content) | ✔ | query, operation name, variables |
| Translation files served at `/i18n/{format}/{file}` | ✔ | format, file |
| Requests with a token or session, mutations, responses with errors | ✘ | |

Responses carry an `X-Tess-Cache` header: `HIT`, `MISS`, or `BYPASS` when the
cache server is unavailable.

Drafts are never cached: anonymous visitors can only read published content,
and signed-in requests always go to the database, so editors and previews
always see the latest version.

## Invalidation

You never purge the cache by hand. Every change invalidates it:

- **Content**: creating, updating, publishing, unpublishing or deleting an
  entry (from the admin UI, the API, MCP, plugins or imports), a change to a
  content model or component, and any successful mutation.
- **Translations**: any change to languages, keys or texts.

Invalidation is instant and costs one command: each scope has a generation
counter that is part of every key. Changing it makes all older entries
unreachable, and they expire on their TTL.

## Several instances

With a cache server, all Tess instances share the cache: a response cached by
one is served by the others, and a change on any instance invalidates it for
all.

The server also carries the **content model changes** (pub/sub): when one
instance changes a model, the others rebuild their GraphQL schema at once
instead of after the `TESS_SCHEMA_REFRESH_SECS` delay.

## Sizing and eviction

Give the server a memory limit and let it evict the least recently used keys:
`--maxmemory 256mb --maxmemory-policy allkeys-lru` (Valkey, Redis, KeyDB), or
`--cache_mode=true --maxmemory=256mb` (Dragonfly). `volatile-lru` works too.
Persistence is not needed: an empty cache only means a few slower requests.

Any eviction policy is safe. If a server evicts an invalidation counter, Tess
restarts it from the current time, never from a value it had before, so
entries cached earlier can't come back.

`TESS_CACHE_TTL_SECS` is capped at half of `TESS_MEDIA_URL_TTL_SECS` and of
`TESS_ID_TTL`, so cached responses never contain expired file URLs or ids.

## When the cache server is down

Tess keeps working without it. Requests skip the cache (`X-Tess-Cache: BYPASS`)
and a warning is logged. After a failure Tess waits 5 seconds before trying
again, so requests don't wait on an unresponsive server. The cache is used
again as soon as the server answers, and the other instances' schemas are
refreshed in case a change was missed.

## Memory

`TESS_CACHE=memory` keeps the cache in the process (at most
`TESS_CACHE_MAX_ENTRIES` responses). It only suits a single instance: other
instances would serve stale responses until the TTL.

## AWS Lambda

Use a cache server (ElastiCache or Upstash) so that all concurrent Lambda
instances share the cache and its invalidation.
