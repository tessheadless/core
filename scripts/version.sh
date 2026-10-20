#!/usr/bin/env bash
# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

# The app version lives in VERSION (`vX.Y.Z`, optionally `vX.Y.Z-rc.1`). This
# script writes it into every sub-project:
#   - Cargo.toml (workspace version, used by all crates/*), the `tess-*` path
#     dependencies of crates/*/Cargo.toml, and Cargo.lock;
#   - package.json and package-lock.json of admin, website, docs and
#     integrations/n8n-nodes-tess.
#
#   scripts/version.sh                  # print the version
#   scripts/version.sh v1.2.0           # set it (the `v` is optional)
#   scripts/version.sh patch|minor|major
#   scripts/version.sh --check          # fail if a sub-project differs (CI)
#
# Then commit, and tag the release with the same `vX.Y.Z`.
set -euo pipefail
cd "$(dirname "$0")/.."

NPM_PROJECTS=(admin website docs integrations/n8n-nodes-tess)
SEMVER='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'

die() { echo "version: $*" >&2; exit 1; }

current() {
  local v
  v=$(tr -d ' \n\r' < VERSION 2>/dev/null) || die "VERSION is missing"
  [[ ${v#v} =~ $SEMVER ]] || die "VERSION must look like v1.2.3, not '$v'"
  echo "${v#v}"
}

bump() { # part version
  local core=${2%%-*} major minor patch
  # A prerelease bumps to its own release (1.3.0-rc.2 → 1.3.0 for patch).
  if [[ $2 == *-* && $1 == patch ]]; then
    echo "$core"
    return
  fi
  IFS=. read -r major minor patch <<<"$core"
  case $1 in
    major) echo "$((major + 1)).0.0" ;;
    minor) echo "$major.$((minor + 1)).0" ;;
    patch) echo "$major.$minor.$((patch + 1))" ;;
  esac
}

# The version of each sub-project, one `path: version` per line.
versions() {
  echo "Cargo.toml: $(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)"
  grep -HoE '^tess-[a-z0-9-]+ = \{ version = "[^"]+"' crates/*/Cargo.toml |
    sed -E 's/^([^:]+):(tess-[a-z0-9-]+) = \{ version = "([^"]+)"/\1 (\2): \3/'
  for p in "${NPM_PROJECTS[@]}"; do
    echo "$p/package.json: $(node -p "require('./$p/package.json').version ?? '(none)'")"
  done
}

set_version() { # version
  local v=$1
  echo "v$v" > VERSION
  # Workspace version, inherited by every crate (version.workspace = true).
  sed -i.bak "/^\[workspace.package\]/,/^\[/s/^version = \".*\"/version = \"$v\"/" Cargo.toml
  # Path dependencies between our crates carry the version too.
  for f in crates/*/Cargo.toml; do
    sed -i.bak -E "s/^(tess-[a-z0-9-]+ = \{ version = )\"[^\"]+\"/\1\"$v\"/" "$f"
  done
  rm -f Cargo.toml.bak crates/*/Cargo.toml.bak
  for p in "${NPM_PROJECTS[@]}"; do
    # package.json and the root entries of package-lock.json, edited as text so
    # the rest of the file (Prettier's formatting included) stays as is.
    node - "$p" "$v" <<'EOF'
const fs = require("node:fs");
const [dir, version] = process.argv.slice(2);
/** Sets `"version"` in the object whose keys are indented by `indent`, after its "name". */
function setIn(text, indent) {
  const versionLine = new RegExp(`^${indent}"version": "[^"]*"`, "m");
  if (versionLine.test(text)) return text.replace(versionLine, `${indent}"version": "${version}"`);
  const nameLine = new RegExp(`^${indent}"name": "[^"]*",\\n`, "m");
  if (!nameLine.test(text)) throw new Error(`${dir}: no "name" to put the version after`);
  return text.replace(nameLine, (line) => `${line}${indent}"version": "${version}",\n`);
}
const pkg = `${dir}/package.json`;
fs.writeFileSync(pkg, setIn(fs.readFileSync(pkg, "utf8"), "  "));
const lock = `${dir}/package-lock.json`;
if (fs.existsSync(lock)) {
  let text = setIn(fs.readFileSync(lock, "utf8"), "  ");
  // The root package entry: `"packages": { "": { "name": …, "version": … } }`.
  const start = text.indexOf('\n    "": {\n');
  const end = text.indexOf("\n    },", start);
  if (start >= 0 && end > start) text = text.slice(0, start) + setIn(text.slice(start, end), "      ") + text.slice(end);
  fs.writeFileSync(lock, text);
}
EOF
  done
  # Last: Cargo.lock entries of our crates.
  if command -v cargo >/dev/null; then
    cargo update --workspace --offline --quiet 2>/dev/null || cargo update --workspace --quiet
  else
    echo "cargo not found: Cargo.lock is updated by the next cargo build" >&2
  fi
}

case ${1:-} in
  "") echo "v$(current)" ;;
  --check)
    v=$(current)
    stale=$(versions | grep -v ": $v\$" || true)
    if [[ -n $stale ]]; then
      echo "Not at v$v (run scripts/version.sh v$v):" >&2
      echo "$stale" >&2
      exit 1
    fi
    echo "All sub-projects at v$v"
    ;;
  major | minor | patch)
    v=$(bump "$1" "$(current)")
    set_version "$v"
    echo "v$v"
    ;;
  -h | --help) sed -n '/^# The app version/,/^# Then commit/s/^# \{0,1\}//p' "$0" ;;
  *)
    v=${1#v}
    [[ $v =~ $SEMVER ]] || die "expected vX.Y.Z (or X.Y.Z, patch, minor, major), not '$1'"
    set_version "$v"
    echo "v$v"
    ;;
esac
