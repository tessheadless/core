#!/bin/sh
# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

# One-time setup of a single-node Garage through its admin API: layout, the
# Tess bucket and its access key. Idempotent, so it runs on every start of the
# `garage-init` compose service (profiles `s3` and `local`). Needs only sh and
# curl; reads the same settings as Tess:
#
#   GARAGE_ADMIN_URL (default http://garage:3903), GARAGE_ADMIN_TOKEN,
#   TESS_S3_BUCKET, TESS_S3_ACCESS_KEY_ID (GK + 24 hex), TESS_S3_SECRET_ACCESS_KEY (64 hex)
set -eu
url=${GARAGE_ADMIN_URL:-http://garage:3903}
: "${GARAGE_ADMIN_TOKEN:?set GARAGE_ADMIN_TOKEN}" "${TESS_S3_BUCKET:?set TESS_S3_BUCKET}"
: "${TESS_S3_ACCESS_KEY_ID:?set TESS_S3_ACCESS_KEY_ID}" "${TESS_S3_SECRET_ACCESS_KEY:?set TESS_S3_SECRET_ACCESS_KEY}"

# api ENDPOINT [JSON]: sets $status (HTTP code) and $body.
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
api() {
  status=$(curl -s -o "$tmp" -w '%{http_code}' -H "Authorization: Bearer $GARAGE_ADMIN_TOKEN" \
    -H 'content-type: application/json' "$url/v2/$1" ${2:+-d "$2"}) || status=000
  body=$(cat "$tmp")
}
# First `"key": "value"` (or number) of $body.
field() { printf '%s' "$body" | sed -n "s/.*\"$1\": *\"\{0,1\}\([^\",]*\)\"\{0,1\}.*/\1/p" | head -n 1; }
must() { [ "$status" = 200 ] || { echo "garage: $1 failed (HTTP $status): $body" >&2; exit 1; }; }

i=0
until api GetClusterStatus && [ "$status" = 200 ]; do
  i=$((i + 1))
  [ $i -lt 60 ] || { echo "garage: admin API unreachable at $url (HTTP $status)" >&2; exit 1; }
  sleep 1
done

if [ "$(field layoutVersion)" = 0 ]; then
  node=$(field id)
  api UpdateClusterLayout "{\"roles\":[{\"id\":\"$node\",\"zone\":\"dc1\",\"capacity\":1000000000,\"tags\":[]}]}"
  must "layout"
  api ApplyClusterLayout '{"version":1}'
  must "layout"
  echo "garage: layout applied"
fi

api "GetKeyInfo?id=$TESS_S3_ACCESS_KEY_ID"
if [ "$status" != 200 ]; then
  api ImportKey "{\"accessKeyId\":\"$TESS_S3_ACCESS_KEY_ID\",\"secretAccessKey\":\"$TESS_S3_SECRET_ACCESS_KEY\",\"name\":\"tess\"}"
  must "key import (TESS_S3_ACCESS_KEY_ID must be GK + 24 hex, the secret 64 hex)"
  echo "garage: key imported"
fi

api "GetBucketInfo?globalAlias=$TESS_S3_BUCKET"
if [ "$status" != 200 ]; then
  api CreateBucket "{\"globalAlias\":\"$TESS_S3_BUCKET\"}"
  must "bucket creation"
  echo "garage: bucket $TESS_S3_BUCKET created"
fi
bucket=$(field id)

api AllowBucketKey "{\"bucketId\":\"$bucket\",\"accessKeyId\":\"$TESS_S3_ACCESS_KEY_ID\",\"permissions\":{\"read\":true,\"write\":true,\"owner\":true}}"
must "bucket permissions"
echo "garage: ready (bucket $TESS_S3_BUCKET)"
