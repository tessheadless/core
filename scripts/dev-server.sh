#!/usr/bin/env bash
# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

# Starts a throwaway CMS (in-memory database, local media under a temp dir)
# with freshly generated secrets, for trying the examples.
#
#   scripts/dev-server.sh            # port 8080
#   PORT=18090 scripts/dev-server.sh
#
# Secrets are written to a private file outside the repository and sourced by
# the examples' helper (`source "$TESS_DEV_ENV"`); nothing is printed.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${TESS_DEV_ENV:=${XDG_RUNTIME_DIR:-/tmp}/tess-dev.env}"
if [[ ! -f "$TESS_DEV_ENV" ]]; then
  umask 077
  {
    echo "export TESS_ADMIN_TOKEN=$(openssl rand -hex 32)"
    echo "export TESS_ID_SECRET=$(openssl rand -hex 32)"
    echo "export TESS_REF_SECRET=$(openssl rand -hex 32)"
    echo "export TESS_SESSION_SECRET=$(openssl rand -hex 32)"
  } > "$TESS_DEV_ENV"
fi
# Env files created before user accounts existed lack the session secret.
grep -q TESS_SESSION_SECRET "$TESS_DEV_ENV" || echo "export TESS_SESSION_SECRET=$(openssl rand -hex 32)" >> "$TESS_DEV_ENV"
# shellcheck source=/dev/null
source "$TESS_DEV_ENV"
export PORT="${PORT:-8080}" TESS_DATABASE="${TESS_DATABASE:-memory}" TESS_SEARCH="${TESS_SEARCH:-memory}"
export TESS_MEDIA_STORE=fs TESS_MEDIA_DIR="${TESS_MEDIA_DIR:-$(mktemp -d)}"
BIN=target/debug/tess
[[ -x $BIN ]] || cargo build -p tess-server
exec "$BIN"
