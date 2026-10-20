---
sidebar_position: 4
title: Databases
---

# Databases

Select a backend with `TESS_DATABASE`. Every backend passes the same
conformance test suite, so filters, sorting, pagination, draft/publish,
users and settings behave identically on all of them. Tables, collections and
indexes are created automatically at startup. Migrations are idempotent and
safe to run concurrently; on PostgreSQL they are serialized with an advisory
lock.

| Backend | `TESS_DATABASE` | Instances | Filtering | Best for |
|---|---|---|---|---|
| PostgreSQL | `postgres` | many | in SQL | general production use |
| MySQL 8 | `mysql` | many | in SQL | existing MySQL infrastructure |
| SQLite | `sqlite` | **one** | in SQL | single-container installs |
| MongoDB | `mongodb` | many | in MongoDB | existing MongoDB infrastructure |
| DynamoDB | `dynamodb` | many | in memory (per type) | AWS Lambda, serverless |
| RocksDB | `rocksdb` | **one** | in memory (per type) | embedded, zero-dependency |
| Memory | `memory` | one | in memory | tests and demos; data is lost on restart |

## SQL: PostgreSQL, MySQL, SQLite

One backend built with [SeaQuery](https://github.com/SeaQL/sea-query) serves
all three. `DATABASE_URL` selects the engine, and its scheme must match
`TESS_DATABASE`.

```bash
TESS_DATABASE=postgres DATABASE_URL=postgres://tess:CHANGE_ME@db.example.com:5432/tess
TESS_DATABASE=mysql    DATABASE_URL='mysql://tess:CHANGE_ME@db.example.com:3306/tess?ssl-mode=required'
TESS_DATABASE=sqlite   DATABASE_URL=sqlite:///data/tess.db
```

Tables: `tess_content_type`, `tess_entry` (one JSON document per stage) and
`tess_setting` (users, API tokens, plugins). The pool size is
`TESS_DB_MAX_CONNECTIONS`.

:::warning MySQL 8 requires TLS
MySQL 8's default authentication (`caching_sha2_password`) needs either TLS
or RSA key exchange, and Tess doesn't enable the RSA option. Add
`?ssl-mode=required` to the URL.
:::

- **SQLite** runs in WAL mode with a busy timeout. Keep the file on a
  persistent volume, and run a single instance.
- **Collations**: string comparison and sorting are bytewise on every engine,
  whatever the database's default collation. Key columns use `utf8mb4_bin`
  on MySQL.
- **Performance**: filters are evaluated on the JSON documents of one content
  type, without dedicated indexes. This is fine for most sites; very large
  types will benefit from future generated indexes.

## MongoDB

```bash
TESS_DATABASE=mongodb MONGODB_URL=mongodb://mongo.example.com:27017 MONGODB_DATABASE=tess
```

Collections: `tess_content_types`, `tess_entries`, `tess_settings`. Filters
and sorting run in MongoDB.

## DynamoDB

Single-table design:

| `pk` | `sk` | `doc` |
|---|---|---|
| `CT` | content type apiId | content type JSON |
| `E#{apiId}` | entry uuid | entry JSON |
| `S` | setting key | setting JSON |

```bash
TESS_DATABASE=dynamodb DYNAMODB_TABLE=tess
```

The table needs string keys `pk` (partition) and `sk` (sort). On-demand
billing is recommended; the [SAM template](aws-lambda.md) creates it. The
IAM role needs CRUD access to the table (`GetItem`, `PutItem`, `DeleteItem`,
`Query`, `BatchWriteItem`; plus `DescribeTable`/`CreateTable` if you use
`DYNAMODB_CREATE_TABLE`).

:::info
List queries read the whole partition of a content type, then filter and sort
in memory. That suits thousands of entries per type; beyond that, put a cache
or CDN in front or choose a SQL backend. Items are limited to 400 KB by
DynamoDB.
:::

## RocksDB

```bash
TESS_DATABASE=rocksdb ROCKSDB_PATH=/data/rocksdb
```

An embedded key-value store: no service to run, one process only (the
directory is locked), and not usable on Lambda. Filtering is done in memory
per content type.

## Moving between backends

There's no built-in migration tool between backends yet. Choose the backend
before going to production.
