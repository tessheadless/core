---
sidebar_position: 1
title: Overview
---

# Deploying Tess

Tess is **one binary**, `tess`, that serves the GraphQL API, the REST upload
endpoints, the health check and the admin UI. When it detects the AWS Lambda
runtime (`AWS_LAMBDA_RUNTIME_API` is set), it runs as a Lambda function;
otherwise it starts an HTTP server on `PORT` (default `8080`).

```text
GET/POST /graphql                 GraphQL API (GraphiQL IDE on GET, if enabled)
GET      /graphql/schema.graphql  current schema (SDL)
POST     /media/upload            file upload (Contributor or above)
GET      /media/files/{key}       file download (filesystem media store)
POST     /api/plugins/{name}      plugin module upload (Admin or above)
GET      /admin/                  admin UI
GET      /health                  health check
GET      /favicon.svg             the Tess logo
```

Everything else is pluggable and chosen by configuration:

| Concern | Options | Page |
|---|---|---|
| Database | PostgreSQL, MySQL 8, SQLite, MongoDB, DynamoDB, RocksDB, memory | [Databases](databases.md) |
| Media files | local filesystem, S3-compatible (Garage, MinIO, AWS S3, R2…) | [Media storage](media-storage.md) |
| Full-text search | none, Typesense, memory | [Search](search.md) |
| Plugins | on/off, sandbox limits | [Security](security.md), [Plugins](../plugins/overview.md) |

## Choosing a deployment

| Deployment | Good for | Typical stack |
|---|---|---|
| **Docker / Compose** | most self-hosted installs | `tess` + PostgreSQL (or SQLite) + Garage + Typesense. See [Docker](docker.md). |
| **Single VM** | small installs | the `tess` binary under systemd, SQLite or RocksDB, filesystem media |
| **Kubernetes** | several replicas | the Docker image as a Deployment, with PostgreSQL/MySQL/MongoDB and S3 |
| **AWS Lambda** | serverless, pay per use | DynamoDB + S3 (+ Typesense Cloud). See [AWS Lambda](aws-lambda.md). |
| **Phusion Passenger** | hosts that only run Ruby/Node apps | `tess` started and proxied by a Rack `config.ru`, RocksDB/SQLite or a managed database. See [Phusion Passenger](passenger.md). |

:::warning Single-instance backends
**RocksDB**, **SQLite** and the **filesystem** media store live on a local
disk. Use them with exactly one instance, and with a persistent volume. Tess
refuses to start with them on AWS Lambda. For several replicas, use a
network database and an S3-compatible bucket.
:::

## Multiple instances

Instances share nothing but the database, the media store and the search
engine. When an admin changes a content model or a plugin, the instance that
handled the request applies it immediately. The others pick it up within
`TESS_SCHEMA_REFRESH_SECS` (default 10 s), which also applies to separate
Lambda execution environments.

## Next steps

1. [Configuration reference](configuration.md)
2. [Docker](docker.md), [AWS Lambda](aws-lambda.md) or [Phusion Passenger](passenger.md)
3. [Security checklist](security.md)
4. [Operations](operations.md): health, logs, backups, upgrades
