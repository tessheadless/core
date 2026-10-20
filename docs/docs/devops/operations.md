---
sidebar_position: 9
title: Operations
---

# Operations

## Health check

```bash
curl -s http://localhost:8080/health
# {"status":"ok","storage":"postgres"}
```

The endpoint returns `200` when the database answers, and `503` otherwise.
Use it for container health checks and load-balancer probes.

## Logs

- **Format**: text logs by default; set `TESS_LOG_FORMAT=json` for
  structured logs. Logs are always JSON on Lambda.
- **Verbosity**: control it with `RUST_LOG`, e.g. `RUST_LOG=info,tower_http=debug`.
- **Notable events**: schema rebuilds, plugin loads and failures, storage
  errors (clients only see "internal storage error"), and account lockouts.

## Backups

Tess keeps **everything in two places**. Back up both, consistently:

1. **The database**: content types, entries, users, API tokens, plugin
   settings.
2. **The media store**: uploaded files and plugin modules.

| Backend | Suggested backup |
|---|---|
| PostgreSQL | `pg_dump`, or managed snapshots / PITR |
| MySQL | `mysqldump --single-transaction`, or managed snapshots |
| SQLite | `sqlite3 tess.db ".backup backup.db"` (safe while running) |
| MongoDB | `mongodump`, or managed snapshots |
| DynamoDB | point-in-time recovery (enabled by the SAM template) |
| RocksDB | stop Tess and copy the directory, or snapshot the volume |
| S3 / Garage | bucket versioning or replication |
| Filesystem media | copy or snapshot `TESS_MEDIA_DIR` |

To copy only the content models and users to another instance (e.g. from
staging to production), use [`tess-transfer`](import-export.md).

After restoring, run `reindexSearch` if you use Typesense (see
[Search](search.md)).

## Upgrades

1. Back up.
2. Deploy the new version.

Schema migrations run automatically at startup and are idempotent.

## Scaling out

Run several instances against a shared network database and an S3 bucket.
Model and plugin changes reach every instance within
`TESS_SCHEMA_REFRESH_SECS`. RocksDB, SQLite and filesystem media are
single-instance only.

## Testing your build

The repository contains the full test suite, including conformance tests
against real PostgreSQL, MySQL, MongoDB, DynamoDB Local, Garage and
Typesense started in throwaway containers:

```bash
scripts/test-all.sh                   # needs Docker
NO_CONTAINERS=1 scripts/test-all.sh   # without external services
```
