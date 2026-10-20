---
sidebar_position: 6.5
title: Cache
---

# Cache

Tess can cache public API responses in [Valkey](https://valkey.io) (or any
Redis-compatible server: Redis, KeyDB, Dragonfly, Amazon ElastiCache, Upstash…).
The cache is optional: with `TESS_CACHE=none` (the default) nothing changes.

```bash
TESS_CACHE=valkey
VALKEY_URL=redis://valkey:6379        # rediss:// for TLS; redis://user:password@host:6379/0
TESS_CACHE_TTL_SECS=60
TESS_CACHE_PREFIX=tess:               # to share one server between installs
```

With Docker Compose, the `cache` profile runs Valkey:

```bash
docker compose --profile cache up -d
```

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

With Valkey, all Tess instances share the cache: a response cached by one is
served by the others, and a change on any instance invalidates it for all.

Valkey also carries the **content model changes**: when one instance changes a
model, the others rebuild their GraphQL schema at once instead of after the
`TESS_SCHEMA_REFRESH_SECS` delay.

## Sizing and eviction

Give the server a memory limit and an eviction policy that only evicts keys
with an expiry, so the invalidation counters are never evicted:

```bash
valkey-server --maxmemory 256mb --maxmemory-policy volatile-lru --save "" --appendonly no
```

The Compose service is configured this way (`VALKEY_MAXMEMORY` sets the limit).
Persistence is not needed: an empty cache only means a few slower requests.

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

Use Valkey (ElastiCache or Upstash) so that all concurrent Lambda instances
share the cache and its invalidation.
