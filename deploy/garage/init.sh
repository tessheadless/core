#!/usr/bin/env sh
# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

# One-time setup of a single-node Garage: layout, bucket and access key.
#
#   deploy/garage/init.sh <container> <bucket> <key-id> <key-secret>
#
# The key id must look like `GK` + 24 hex chars and the secret 64 hex chars:
#   key_id="GK$(openssl rand -hex 12)"; key_secret="$(openssl rand -hex 32)"
# Idempotent: re-running it on an initialized node is harmless.
set -eu
container=$1 bucket=$2 key_id=$3 key_secret=$4
g() { docker exec "$container" /garage "$@"; }

for _ in $(seq 30); do g status >/dev/null 2>&1 && break; sleep 1; done
node=$(g status 2>/dev/null | awk '/^[0-9a-f]{16}/ {print $1; exit}')
if g layout show 2>/dev/null | grep -q "No nodes currently have a role"; then
  g layout assign -z dc1 -c 1G "$node"
  g layout apply --version 1
fi
g bucket info "$bucket" >/dev/null 2>&1 || g bucket create "$bucket"
g key info "$key_id" >/dev/null 2>&1 || g key import --yes -n tess "$key_id" "$key_secret"
g bucket allow --read --write --owner "$bucket" --key "$key_id"
echo "garage ready: bucket=$bucket"
