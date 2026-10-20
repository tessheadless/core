---
sidebar_position: 1
title: Introduction
slug: /intro
---

# Tess

**Tess** (from *tessera*, a single tile of a mosaic) is a self-hostable,
GraphQL-first **headless CMS** written in Rust, in the spirit of Strapi and
Hygraph. Every content entry is a tile; your websites, apps and services are
the mosaic.

## What you get

- **Content models defined at runtime.** Create content types in the admin UI
  or through the API, and Tess regenerates its GraphQL schema on the fly:
  queries, filters, sorting, pagination, relations and mutations.
- **Draft and publish** on every entry, plus an optional **review workflow**
  per content type: submit, then approve or request changes.
- **Users and roles**: Owner, Admin, Contributor, Reviewer and Member, each
  user optionally limited to some content types. **API tokens** cover CI and
  front-end builds.
- **Any database.** PostgreSQL, MySQL and SQLite (one SeaQuery-based
  backend), MongoDB, DynamoDB, embedded RocksDB, or in-memory. All of them
  pass the same conformance suite, so they behave identically.
- **Media library** on S3-compatible storage (Garage, MinIO, AWS S3, R2…) or
  the local filesystem, with direct presigned uploads.
- **Full-text search** with Typesense.
- **Plugins**: sandboxed WebAssembly modules that admins upload and configure
  at runtime. You can write them in Rust, Go, JavaScript and more.
- **Opaque, expiring, encrypted ids** in the API; database ids never leave the
  server.
- **One binary, two runtimes**: an HTTP server for Docker, VMs or Kubernetes,
  or an **AWS Lambda** function. The runtime is detected automatically.

## How it fits together

```text
               ┌──────────────── tess (axum | lambda_http) ──────────────────┐
 clients ──▶   │ /graphql  /media/*  /api/plugins/*  /admin/  /health        │
               │        tess-graphql: dynamic schema, auth, opaque ids       │
               │        tess-core: models, validation, roles, workflow, hooks│
               │   hooks ─▶ search (Typesense) · media · WebAssembly plugins │
               └───────┬──────────────────────┬────────────────────┬─────────┘
                 Storage trait          BlobStore trait       SearchEngine trait
     sql (postgres│mysql│sqlite)│      s3 (Garage…)│fs        typesense│memory
     mongodb│dynamodb│rocksdb│memory
```

## Where to go next

| You want to… | Read |
|---|---|
| Try Tess in five minutes | [Quick start](getting-started/quick-start.md) |
| Understand models, stages, roles and ids | [Concepts](getting-started/concepts.md) |
| Install, configure and run it in production | [Deploy and operate](devops/overview.md) |
| Query and write content from your app | [Build with the API](developers/overview.md) |
| Extend Tess with your own logic | [Plugins](plugins/overview.md) |
| Know what the Community and Enterprise editions include | [Editions](editions.md) |
