---
sidebar_position: 5
title: Media storage
---

# Media storage

Uploaded files (and plugin modules) are stored in a **blob store**. Their
metadata lives in the database, as entries of the built-in `Asset` content
type.

## Filesystem

```bash
TESS_MEDIA_STORE=fs TESS_MEDIA_DIR=/data/media
```

Files are served by Tess at `/media/files/{key}`. Keys contain 128 random
bits (`assets/<random>/<file-name>`), and responses carry
`X-Content-Type-Options: nosniff` and `Content-Security-Policy: default-src
'none'; sandbox`, so uploaded HTML or SVG can't run scripts in your origin.
Use this store with **one instance** only, on a persistent volume.

## S3-compatible (Garage, MinIO, AWS S3, Cloudflare R2…)

```bash
TESS_MEDIA_STORE=s3
TESS_S3_BUCKET=tess-media
TESS_S3_ENDPOINT=http://garage:3900       # omit for AWS S3
TESS_S3_REGION=garage
TESS_S3_ACCESS_KEY_ID=GK…                 # omit both to use the AWS chain (IAM role)
TESS_S3_SECRET_ACCESS_KEY=…
```

When `TESS_S3_ENDPOINT` is set, path-style URLs are used
(`TESS_S3_FORCE_PATH_STYLE`), as most self-hosted services require. To
create the bucket at startup, set `TESS_S3_CREATE_BUCKET=true`.

File URLs are presigned for the endpoint. When Tess reaches the store through
an internal address that browsers can't (`http://garage:3900` inside Docker
Compose), set `TESS_S3_PUBLIC_ENDPOINT` to the address browsers use, e.g.
`http://localhost:3900` or `https://s3.example.com`. Tess keeps talking to
`TESS_S3_ENDPOINT`, and signs download and direct-upload URLs for the public
one.

### Direct uploads

With S3 stores, clients can upload straight to the bucket with a presigned
`PUT` (`createAssetUpload` → upload → `confirmAssetUpload`, see
[Media for developers](../developers/media.md)). This avoids proxying large
files through Tess, and bypasses the 6 MB request limit of AWS Lambda.
Browsers then need a **CORS rule on the bucket**, for example:

```json
[{ "AllowedMethods": ["PUT", "GET"], "AllowedOrigins": ["https://admin.example.com"],
   "AllowedHeaders": ["*"], "MaxAgeSeconds": 3600 }]
```

### Download URLs

`Asset.url` is computed in this order:

1. `TESS_MEDIA_PUBLIC_URL` is set (a CDN or public bucket): the URL is
   `{TESS_MEDIA_PUBLIC_URL}/{key}`.
2. Otherwise, the store supports presigning: a presigned `GET` URL, valid
   `TESS_MEDIA_URL_TTL_SECS` seconds.
3. Otherwise: `/media/files/{key}`, served by Tess.

## Limits and lifecycle

- **Size limit:** files over `TESS_MEDIA_MAX_UPLOAD_BYTES` (default 100 MiB)
  are rejected.
- **Deletion:** deleting an `Asset` entry deletes its file.
- **Plugin modules** are stored in the same store under `plugins/`, with
  their SHA-256 checksum verified on load. Back them up together with your
  database (see [Operations](operations.md)).
