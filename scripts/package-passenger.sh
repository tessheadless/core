#!/usr/bin/env bash
# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

# Builds a folder ready to upload to a Phusion Passenger host:
#
#   dist/passenger/
#   ├── config.ru  Gemfile   Rack launcher + proxy (deploy/passenger)
#   ├── tess                 release binary
#   ├── public/admin/        admin UI (static, served by Passenger)
#   ├── log/  tmp/  data/
#
#   scripts/package-passenger.sh                      # host target
#   TARGET=x86_64-unknown-linux-gnu FEATURES=sql,rocksdb,s3 scripts/package-passenger.sh
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=${OUT:-dist/passenger}
FEATURES=${FEATURES:-sql,rocksdb,s3}
cargo_args=(build --release --locked -p tess-server --no-default-features --features "$FEATURES")
bin_dir=target/release
if [[ -n "${TARGET:-}" ]]; then
  cargo_args+=(--target "$TARGET")
  bin_dir=target/$TARGET/release
fi

echo "==> building tess ($FEATURES)"
cargo "${cargo_args[@]}"
echo "==> building the admin UI"
(cd admin && npm ci --no-audit --no-fund && npm run build)

echo "==> assembling $OUT"
rm -rf "$OUT"
mkdir -p "$OUT/public" "$OUT/log" "$OUT/tmp" "$OUT/data"
# build.rs put config.ru and Gemfile next to the binary.
cp "$bin_dir/tess" "$bin_dir/config.ru" "$bin_dir/Gemfile" "$OUT/"
cp -r admin/dist/client "$OUT/public/admin"
tar -C "$(dirname "$OUT")" -czf "$OUT.tar.gz" "$(basename "$OUT")"
echo "==> done: $OUT and $OUT.tar.gz"
