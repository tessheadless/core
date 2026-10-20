---
sidebar_position: 6
title: Media
---

# Media

Uploaded files are entries of the built-in **`Asset`** content type:

| Field | |
|---|---|
| `fileName`, `mimeType`, `size` | file metadata |
| `key` | storage key |
| `status` | `PENDING` (direct upload not confirmed yet) or `READY` |
| `alt`, `width`, `height` | optional, for your front-end |
| `url` | where to download the file |

To add media to your models, use a relation to `Asset`
(`{ name: "cover", type: RELATION, relationTarget: "Asset" }`). Uploading
requires the Contributor role or above. Deleting an asset deletes its file.

## Upload through Tess

Works with every media store:

```bash
curl -s -X POST "https://cms.example.com/media/upload?fileName=cat.jpg" \
  -H "authorization: Bearer $TOKEN" \
  -H 'content-type: image/jpeg' \
  --data-binary @cat.jpg
```

The response is the asset as JSON (`id`, `ref`, `fileName`, `mimeType`,
`size`, `url`, …), with status `201`. Errors use the
[REST error shape](errors.md#rest-endpoints).

## Direct upload to the bucket

With an S3-compatible store, clients can upload straight to the bucket. This
is best for large files, and required beyond 6 MB on AWS Lambda.

```graphql
mutation {
  createAssetUpload(fileName: "video.mp4", mimeType: "video/mp4", size: 52428800) {
    asset { id status }      # PENDING
    url method expiresIn
    headers { name value }   # send them exactly
  }
}
```

```bash
curl -X PUT "$URL" -H "content-type: video/mp4" --data-binary @video.mp4
```

```graphql
mutation($id: ID!) { confirmAssetUpload(id: $id) { status size url } }
```

`confirmAssetUpload` checks that the object exists and is within the size
limit, then marks the asset `READY`. With the filesystem store,
`createAssetUpload` returns `BAD_USER_INPUT`: use `/media/upload`.

## Download URLs

`Asset.url` is either a CDN URL, a presigned URL (valid for
`TESS_MEDIA_URL_TTL_SECS`) or a `/media/files/…` path served by Tess,
depending on the [server configuration](../devops/media-storage.md).
Presigned URLs expire, so fetch `url` when you render, don't store it.
