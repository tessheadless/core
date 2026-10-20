#!/usr/bin/env bash
# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

# Full test suite: unit + API tests, and the storage/media/search/plugin
# conformance tests against real services started in throwaway containers
# (Postgres, MySQL, MongoDB, DynamoDB Local, Garage, Typesense). SQLite needs
# no service and always runs.
#
#   scripts/test-all.sh            # everything
#   NO_CONTAINERS=1 scripts/test-all.sh   # only what needs no services
set -euo pipefail
cd "$(dirname "$0")/.."
P=tess-it
cleanup() { docker rm -f $P-pg $P-mysql $P-mongo $P-ddb $P-garage $P-typesense >/dev/null 2>&1 || true; }

echo "==> building example plugins"
(cd examples/plugins/rust-slugify && cargo build -q --release --target wasm32-unknown-unknown)
(cd examples/plugins/rust-webhook && cargo build -q --release --target wasm32-unknown-unknown)
(cd examples/plugins/go-reading-time && GOOS=wasip1 GOARCH=wasm go build -buildmode=c-shared -o reading-time.wasm .)
export TESS_TEST_PLUGIN_WASM=$PWD/examples/plugins/rust-slugify/target/wasm32-unknown-unknown/release/slugify_plugin.wasm

if [[ -z "${NO_CONTAINERS:-}" ]]; then
  trap cleanup EXIT
  cleanup
  echo "==> starting services"
  pg_pass=$(openssl rand -hex 16)
  ts_key=$(openssl rand -hex 16)
  docker run -d --name $P-pg -e POSTGRES_USER=tess -e POSTGRES_DB=tess -e POSTGRES_PASSWORD="$pg_pass" -p 127.0.0.1:55532:5432 postgres:17-alpine >/dev/null
  docker run -d --name $P-mysql -e MYSQL_USER=tess -e MYSQL_DATABASE=tess -e MYSQL_PASSWORD="$pg_pass" -e MYSQL_RANDOM_ROOT_PASSWORD=yes -p 127.0.0.1:53506:3306 mysql:8.4 >/dev/null
  docker run -d --name $P-mongo -p 127.0.0.1:57117:27017 mongo:8 >/dev/null
  docker run -d --name $P-ddb -p 127.0.0.1:58100:8000 amazon/dynamodb-local:latest >/dev/null
  docker run -d --name $P-typesense -p 127.0.0.1:58208:8108 -e TYPESENSE_API_KEY="$ts_key" -e TYPESENSE_DATA_DIR=/data --tmpfs /data typesense/typesense:29.0 >/dev/null
  docker run -d --name $P-garage -p 127.0.0.1:54900:3900 -e GARAGE_RPC_SECRET="$(openssl rand -hex 32)" \
    -e GARAGE_ADMIN_TOKEN="$(openssl rand -hex 32)" -v "$PWD/deploy/garage/garage.toml:/etc/garage.toml:ro" dxflrs/garage:v2.1.0 >/dev/null
  s3_id="GK$(openssl rand -hex 12)" s3_secret=$(openssl rand -hex 32)
  deploy/garage/init.sh $P-garage tess-test "$s3_id" "$s3_secret" >/dev/null 2>&1
  for _ in $(seq 60); do docker exec $P-pg pg_isready -U tess >/dev/null 2>&1 && break; sleep 1; done
  for _ in $(seq 90); do docker exec $P-mysql mysqladmin ping -ucms -p"$pg_pass" --silent >/dev/null 2>&1 && break; sleep 2; done
  for _ in $(seq 60); do curl -sf http://127.0.0.1:58208/health >/dev/null && break; sleep 1; done

  export TESS_TEST_POSTGRES_URL="postgres://tess:$pg_pass@127.0.0.1:55532/tess?sslmode=disable"
  # MySQL 8 authentication needs TLS (or sqlx's RSA feature, not enabled).
  export TESS_TEST_MYSQL_URL="mysql://tess:$pg_pass@127.0.0.1:53506/tess?ssl-mode=required"
  export TESS_TEST_MONGODB_URL=mongodb://127.0.0.1:57117
  export TESS_TEST_DYNAMODB_ENDPOINT=http://127.0.0.1:58100 AWS_ACCESS_KEY_ID=local AWS_SECRET_ACCESS_KEY=local AWS_REGION=us-east-1
  export TESS_TEST_TYPESENSE_URL=http://127.0.0.1:58208 TESS_TEST_TYPESENSE_KEY="$ts_key"
  export TESS_TEST_S3_ENDPOINT=http://127.0.0.1:54900 TESS_TEST_S3_BUCKET=tess-test TESS_TEST_S3_KEY_ID="$s3_id" TESS_TEST_S3_SECRET="$s3_secret"
fi

echo "==> cargo test"
log=$(mktemp)
if cargo test --workspace --quiet >"$log" 2>&1; then status=0; else status=$?; fi
grep -E "^test result|panicked|^error" "$log" | sort | uniq -c
if [[ $status -eq 0 ]]; then echo "ALL TESTS PASSED"; else tail -50 "$log"; echo "TESTS FAILED"; fi
rm -f "$log"
exit $status
