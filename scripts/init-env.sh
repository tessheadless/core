#!/usr/bin/env bash
# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

# Creates the `.env` of a new install: every setting Tess reads, the chosen
# backends filled in, the others commented out with their defaults, and fresh
# random secrets.
#
#   scripts/init-env.sh                     # asks a few questions
#   scripts/init-env.sh -y                  # defaults: Docker, RocksDB, files on disk
#   scripts/init-env.sh -y --database postgres --search typesense --n8n \
#       --url https://cms.example.com
#   scripts/init-env.sh --target native -o tess.env   # for the binary, no Docker
#
# Options:
#   -o, --output FILE   file to write (default: .env)
#   -f, --force         replace FILE; its secrets are kept (backup in FILE.bak)
#       --new-secrets   with --force: generate new secrets too
#   -y, --yes           no questions, use the defaults and options
#       --target T      docker (compose service names, /data volume) | native
#       --database DB   rocksdb | sqlite | postgres | mysql | mongodb | dynamodb | memory
#       --media M       fs | s3 (docker: the Garage `s3` profile)
#       --search S      none | memory | typesense
#       --cache C       none | memory | valkey (docker: the Valkey `cache` profile)
#       --n8n, --no-n8n enable the n8n workflows (docker: the `n8n` profile)
#       --url URL       public URL of Tess (default: http://localhost:8080)
#       --version V     docker image version (default: latest)
#       --arch A        docker image suffix: amd64 | arm64 | none (default: this host)
#
# Secrets are written to the file only, never printed.
set -euo pipefail

usage() { sed -n '/^# Creates/,/^# Secrets/s/^# \{0,1\}//p' "$0"; exit "${1:-0}"; }
die() { echo "init-env: $*" >&2; exit 1; }

output=.env force=0 new_secrets=0 yes=0
target='' database='' media='' search='' cache='' n8n='' url='' version='' arch=''
while (($#)); do
  case $1 in
    -o | --output) output=${2:?}; shift ;;
    -f | --force) force=1 ;;
    --new-secrets) new_secrets=1 ;;
    -y | --yes) yes=1 ;;
    --target) target=${2:?}; shift ;;
    --database) database=${2:?}; shift ;;
    --media) media=${2:?}; shift ;;
    --search) search=${2:?}; shift ;;
    --cache) cache=${2:?}; shift ;;
    --n8n) n8n=yes ;;
    --no-n8n) n8n=no ;;
    --url) url=${2:?}; shift ;;
    --version) version=${2:?}; shift ;;
    --arch) arch=${2:?}; shift ;;
    -h | --help) usage ;;
    *) echo "init-env: unknown option $1" >&2; usage 2 ;;
  esac
  shift
done

if [[ -e $output && $force != 1 ]]; then
  die "$output exists; use --force to replace it (its secrets are kept)"
fi
# Questions need a terminal; without one, behave like --yes.
[[ -t 0 ]] || yes=1

# ask VAR "Question" default choice...
ask() {
  local name=$1 question=$2 default=$3 answer
  shift 3
  [[ -n ${!name} ]] && return
  if [[ $yes == 1 ]]; then
    printf -v "$name" %s "$default"
    return
  fi
  while :; do
    read -r -p "$question${*:+ (${*// /, })} [$default]: " answer </dev/tty || answer=''
    answer=${answer:-$default}
    if (($# == 0)) || [[ " $* " == *" $answer "* ]]; then
      printf -v "$name" %s "$answer"
      return
    fi
    echo "  choose one of: $*" >&2
  done
}

one_of() { # value name choice...
  local value=$1 name=$2
  shift 2
  [[ " $* " == *" $value "* ]] || die "$name must be one of: $*"
}

ask target "Run with" docker docker native
one_of "$target" --target docker native
ask database "Database" rocksdb rocksdb sqlite postgres mysql mongodb dynamodb memory
one_of "$database" --database rocksdb sqlite postgres mysql mongodb dynamodb memory
ask media "Media files" fs fs s3
one_of "$media" --media fs s3
ask search "Full-text search" none none memory typesense
one_of "$search" --search none memory typesense
ask cache "Response cache" none none memory valkey
one_of "$cache" --cache none memory valkey
ask n8n "n8n workflows" no yes no
one_of "$n8n" --n8n yes no
ask url "Public URL of Tess" http://localhost:8080
url=${url%/}
[[ $url == http://* || $url == https://* ]] || die "--url must start with http:// or https://"
if [[ $target == docker ]]; then
  case $(uname -m) in
    x86_64 | amd64) host_arch=amd64 ;;
    aarch64 | arm64) host_arch=arm64 ;;
    *) host_arch=none ;;
  esac
  ask version "Tess image version" latest
  ask arch "Image architecture" "$host_arch" amd64 arm64 none
  one_of "$arch" --arch amd64 arm64 none
  [[ $arch == none ]] && arch=''
fi

random() { openssl rand -hex "$1" 2>/dev/null || od -An -N"$1" -tx1 /dev/urandom | tr -d ' \n'; }

# The current value of KEY in the file being replaced.
previous() {
  [[ -f $output && $new_secrets != 1 ]] || return 0
  sed -n "s/^$1=//p" "$output" | tail -n 1
}

# secret KEY BYTES: the previous value, else a new random one.
secret() {
  local value
  value=$(previous "$1")
  printf %s "${value:-$(random "$2")}"
}

# Hosts and paths differ between the compose services and a native install.
if [[ $target == docker ]]; then
  data=/data
  host() { echo "$1"; }
else
  data=data
  host() { echo localhost; }
fi
# How to source the file from the current directory.
env_path=$output
[[ $env_path == */* ]] || env_path=./$env_path
https=false
[[ $url == https://* ]] && https=true
is() { [[ $1 == "$2" ]] && echo 1 || echo 0; }

# kv ACTIVE KEY VALUE: `KEY=VALUE`, or commented out when ACTIVE is 0.
kv() { if [[ $1 == 1 ]]; then echo "$2=$3"; else echo "# $2=$3"; fi; }
section() { printf '\n# --- %s %s\n' "$1" "$(printf '%*s' $((${#1} < 72 ? 76 - ${#1} : 3)) '' | tr ' ' -)"; }

pg_password=$(secret POSTGRES_PASSWORD 24)
mysql_password=$(secret MYSQL_PASSWORD 24)
typesense_key=$(secret TYPESENSE_API_KEY 32)
s3_key_id=$(previous TESS_S3_ACCESS_KEY_ID)
s3_key_id=${s3_key_id:-GK$(random 12)}

case $database in
  sqlite) database_url="sqlite://$data/tess.db" ;;
  postgres) database_url="postgres://tess:$pg_password@$(host postgres):5432/tess" ;;
  mysql) database_url="mysql://tess:$mysql_password@$(host mysql):3306/tess?ssl-mode=required" ;;
  *) database_url='' ;;
esac
if [[ $target == docker ]]; then
  profiles=''
  for p in postgres mysql mongodb dynamodb; do [[ $database == "$p" ]] && profiles+=" --profile $p"; done
  [[ $media == s3 ]] && profiles+=' --profile s3'
  [[ $search == typesense ]] && profiles+=' --profile search'
  [[ $cache == valkey ]] && profiles+=' --profile cache'
  [[ $n8n == yes ]] && profiles+=' --profile n8n'
fi

write() {
  cat <<EOF
# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

# Tess configuration, created by scripts/init-env.sh on $(date -u +%Y-%m-%d).
# Keep it private (never commit it) and back it up with your data.
# Commented-out settings show their default value.
EOF
  if [[ $target == docker ]]; then
    cat <<EOF
#
#   docker compose${profiles} up -d
#
# docker-compose-local.yml only reads the secrets, the image and the n8n/MCP
# settings: it always uses RocksDB, files on disk and no search.
EOF
    section "Docker images"
    echo "TESS_IMAGE=$(previous TESS_IMAGE | grep . || echo ghcr.io/tessheadless/tess)"
    echo "TESS_N8N_IMAGE=$(previous TESS_N8N_IMAGE | grep . || echo ghcr.io/tessheadless/tess-n8n)"
    echo "# Image tag: \${TESS_VERSION}-\${TESS_ARCH}, or the multi-arch \${TESS_VERSION} when TESS_ARCH is empty."
    echo "TESS_VERSION=$version"
    echo "TESS_ARCH=$arch"
  else
    cat <<EOF
#
#   set -a && . $env_path && set +a && ./tess
EOF
  fi

  section "Secrets"
  cat <<EOF
# Bootstrap admin token: sign in with it to create the first Owner account,
# then empty it and restart.
TESS_ADMIN_TOKEN=$(secret TESS_ADMIN_TOKEN 32)
# Signs user sessions. Changing it signs everybody out.
TESS_SESSION_SECRET=$(secret TESS_SESSION_SECRET 32)
TESS_SESSION_TTL=12h
# Keys of the opaque API ids and stable refs. Changing TESS_REF_SECRET changes
# every ref; rotate TESS_ID_SECRET by moving the old one to TESS_ID_SECRET_PREVIOUS.
TESS_ID_SECRET=$(secret TESS_ID_SECRET 32)
TESS_REF_SECRET=$(secret TESS_REF_SECRET 32)
# TESS_ID_SECRET_PREVIOUS=
TESS_ID_TTL=24h
# Encrypts the integration secrets stored in the database (webhook secrets,
# storage keys of translation targets). Losing it makes them unreadable.
TESS_SECRETS_KEY=$(secret TESS_SECRETS_KEY 32)
EOF

  section "Server"
  echo "PORT=8080"
  echo "# json for log collectors, text for humans."
  echo "TESS_LOG_FORMAT=$([[ $target == docker ]] && echo json || echo text)"
  echo "# RUST_LOG=info"
  echo "# Admin UI folder (default: admin/dist/client, else admin/ next to the binary)."
  kv 0 TESS_ADMIN_DIR "$([[ $target == docker ]] && echo /app/admin || echo admin)"

  section "Database: rocksdb | sqlite | postgres | mysql | mongodb | dynamodb | memory"
  echo "TESS_DATABASE=$database"
  kv "$(is "$database" rocksdb)" ROCKSDB_PATH "$data/rocksdb"
  echo "# postgres, mysql, sqlite. The scheme must match TESS_DATABASE."
  kv "$([[ -n $database_url ]] && echo 1 || echo 0)" DATABASE_URL "${database_url:-postgres://tess:CHANGE_ME@$(host postgres):5432/tess}"
  kv 0 TESS_DB_MAX_CONNECTIONS 10
  if [[ $target == docker ]]; then
    echo "# Passwords of the compose database services."
    kv "$(is "$database" postgres)" POSTGRES_PASSWORD "$pg_password"
    kv "$(is "$database" mysql)" MYSQL_PASSWORD "$mysql_password"
  fi
  kv "$(is "$database" mongodb)" MONGODB_URL "mongodb://$(host mongo):27017"
  kv "$(is "$database" mongodb)" MONGODB_DATABASE tess
  kv "$(is "$database" dynamodb)" DYNAMODB_TABLE tess
  echo "# DynamoDB Local; leave empty for AWS."
  kv "$(is "$database" dynamodb)" DYNAMODB_ENDPOINT "http://$(host dynamodb):8000"
  kv "$(is "$database" dynamodb)" DYNAMODB_CREATE_TABLE true

  section "Media: fs | s3"
  echo "TESS_MEDIA_STORE=$media"
  kv "$(is "$media" fs)" TESS_MEDIA_DIR "$data/media"
  if [[ $target == docker ]]; then
    echo "# Garage (\`s3\` profile). After \`docker compose --profile s3 up -d\`, once:"
    echo "#   set -a && . $env_path && set +a"
    echo "#   deploy/garage/init.sh \"\$(docker compose ps -q garage)\" \"\$TESS_S3_BUCKET\" \"\$TESS_S3_ACCESS_KEY_ID\" \"\$TESS_S3_SECRET_ACCESS_KEY\""
  else
    echo "# Any S3 API: AWS (no endpoint), Garage, Cloudflare R2, MinIO..."
  fi
  kv "$(is "$media" s3)" TESS_S3_BUCKET tess-media
  kv "$(is "$media" s3)" TESS_S3_ENDPOINT "http://$(host garage):3900"
  kv "$(is "$media" s3)" TESS_S3_REGION garage
  echo "# Both or neither (neither: the AWS credential chain, e.g. an IAM role)."
  kv "$(is "$media" s3)" TESS_S3_ACCESS_KEY_ID "$s3_key_id"
  kv "$(is "$media" s3)" TESS_S3_SECRET_ACCESS_KEY "$(secret TESS_S3_SECRET_ACCESS_KEY 32)"
  echo "# Default: true when TESS_S3_ENDPOINT is set."
  kv 0 TESS_S3_FORCE_PATH_STYLE true
  kv 0 TESS_S3_CREATE_BUCKET false
  if [[ $target == docker ]]; then
    kv "$(is "$media" s3)" GARAGE_RPC_SECRET "$(secret GARAGE_RPC_SECRET 32)"
    kv "$(is "$media" s3)" GARAGE_ADMIN_TOKEN "$(secret GARAGE_ADMIN_TOKEN 32)"
  fi
  echo "# Public URL of the bucket or CDN, when files are publicly readable there."
  kv 0 TESS_MEDIA_PUBLIC_URL https://cdn.example.com
  kv 0 TESS_MEDIA_URL_TTL_SECS 3600
  kv 0 TESS_MEDIA_MAX_UPLOAD_BYTES 104857600

  section "Search: none | memory | typesense"
  echo "TESS_SEARCH=$search"
  kv "$(is "$search" typesense)" TYPESENSE_URL "http://$(host typesense):8108"
  kv "$(is "$search" typesense)" TYPESENSE_API_KEY "$typesense_key"
  kv 0 TYPESENSE_COLLECTION_PREFIX tess_

  section "GraphQL API"
  echo "# Anonymous read access to PUBLISHED content."
  echo "TESS_PUBLIC_READ=true"
  echo "TESS_GRAPHIQL=true"
  kv 0 TESS_INTROSPECTION true
  echo "# Origins allowed by CORS (* = any). In production, list your front ends:"
  echo "# TESS_CORS_ORIGINS=https://www.example.com,https://app.example.com"
  echo "TESS_CORS_ORIGINS=*"
  kv 0 TESS_DEFAULT_PAGE_SIZE 100
  kv 0 TESS_MAX_PAGE_SIZE 1000
  kv 0 TESS_MAX_QUERY_DEPTH 16
  kv 0 TESS_SCHEMA_REFRESH_SECS 10

  section "Cache: none | memory | valkey"
  echo "# Anonymous GraphQL reads and served translation files; invalidated on every change."
  echo "TESS_CACHE=$cache"
  kv "$(is "$cache" valkey)" VALKEY_URL "redis://$(host valkey):6379"
  kv 0 TESS_CACHE_TTL_SECS 60
  kv 0 TESS_CACHE_PREFIX tess:
  kv 0 TESS_CACHE_MAX_ENTRIES 10000

  section "Plugins"
  echo "TESS_PLUGINS=true"
  kv 0 TESS_PLUGIN_ALLOW_HTTP true
  kv 0 TESS_PLUGIN_MAX_TIMEOUT_MS 10000
  kv 0 TESS_PLUGIN_MAX_MODULE_BYTES 20971520
  kv 0 TESS_PLUGIN_MAX_MEMORY_PAGES 1024

  section "Translations (i18n)"
  echo "TESS_I18N=true"

  section "Webhooks"
  echo "TESS_WEBHOOKS=true"
  echo "# Hosts webhooks may target (empty = any), e.g. n8n,*.example.com"
  kv 0 TESS_WEBHOOK_ALLOWED_HOSTS ''
  kv 0 TESS_WEBHOOK_TIMEOUT_SECS 10
  kv 0 TESS_WEBHOOK_MAX_ATTEMPTS 5

  section "Workflows (n8n)"
  echo "# Tess proxies TESS_N8N_PATH to TESS_N8N_URL and shows the editor in the admin UI."
  kv "$(is "$n8n" yes)" TESS_N8N_URL "http://$(host n8n):5678"
  kv 0 TESS_N8N_PATH /n8n/
  if [[ $target == docker ]]; then
    kv "$(is "$n8n" yes)" N8N_ENCRYPTION_KEY "$(secret N8N_ENCRYPTION_KEY 32)"
    echo "# As browsers reach n8n."
    kv "$(is "$n8n" yes)" N8N_PUBLIC_URL "$url/n8n/"
    kv "$(is "$n8n" yes)" N8N_SECURE_COOKIE "$https"
    kv 0 GENERIC_TIMEZONE UTC
  fi

  section "MCP (AI agents)"
  echo "# Serves the MCP endpoint at /mcp."
  echo "TESS_MCP=false"
  echo "TESS_MCP_ALLOWED_HOSTS=localhost,127.0.0.1,::1"
  echo "# Used by \`tess mcp\` (stdio proxy for desktop agents), not by the server."
  kv 0 TESS_MCP_URL "$url"
  kv 0 TESS_MCP_TOKEN ''
}

tmp=$(mktemp "${output}.XXXXXX")
trap 'rm -f "$tmp"' EXIT
chmod 600 "$tmp"
write >"$tmp"
[[ -e $output ]] && cp -p "$output" "$output.bak"
mv "$tmp" "$output"
trap - EXIT

echo "Wrote $output ($target, $database, media $media, search $search, cache $cache, n8n $n8n)."
[[ -e $output.bak ]] && echo "Previous file: $output.bak"
if [[ $target == docker ]]; then
  echo "Start:   docker compose${profiles} up -d"
else
  echo "Start:   set -a && . $env_path && set +a && ./tess"
fi
[[ $media == s3 && $target == docker ]] && echo "Garage:  run deploy/garage/init.sh once (see the Media section of $output)"
echo "Sign in: $url/admin/ with TESS_ADMIN_TOKEN from $output"
