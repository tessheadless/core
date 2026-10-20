# Tess: project guide for AI assistants

Tess is a GraphQL-first headless CMS written in Rust. This file records the
choices made for the project. Follow them unless the maintainer says
otherwise, and update this file when a choice changes.

## Ground rules

- **Never commit, push, tag or publish without an explicit request.** Git
  uses git-flow (AVH): `main` holds releases, `develop` is the working branch,
  releases are tagged `vX.Y.Z`. The remote is `git@github.com:tessheadless/core.git`.
- **Keep the docs in step with the code.** A new feature, setting or
  behavior is documented in the same change (see [Documentation](#documentation)).
- **Verify before claiming.** Build, lint and test what you change, and say
  plainly what was not tested.
- **Ask before touching what isn't yours to change**: the git index, files
  outside the request, running services on the machine.

## Licensing

- Hybrid license, see [LICENSE.md](LICENSE.md): **Community edition** for
  personal use, **Enterprise edition** for commercial use. Copyright "Tess Dev
  Team". Never describe Tess as open source or MIT/Apache-licensed.
- **Every source file starts with this header** (adapt the comment syntax:
  `//`, `#`, `<!-- -->`; after the shebang line in scripts):

  ```
  // Copyright © 2026 Tess Dev Team <https://tessheadless.com>
  // SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
  // Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.
  ```

  Files that can't hold comments (JSON) are exempt. Docs pages (Markdown)
  don't carry it.
- Dependencies keep their own licenses. Prefer permissively licensed ones.
- At runtime, `TESS_LICENSE=community|commercial` (with `TESS_LICENSE_KEY`,
  `tess_…`) selects the license. The **commercial license excludes the embedded
  n8n editor** (n8n's own license forbids embedding it in a commercial product):
  no `/n8n` proxy, no Workflows in the admin, no n8n in the
  `local-commercial` Compose profile. Never log or return the key.
- The **license server** that issues and checks keys lives in a separate,
  private repository. Never add its code, secrets or signing keys here.

## Technologies

| Part | Stack |
|---|---|
| `crates/` | Rust (edition 2024) workspace: axum 0.8, async-graphql (dynamic schema), tokio, sqlx + sea-query, reqwest with rustls, Extism plugins (WebAssembly) |
| Storage backends | PostgreSQL, MySQL, SQLite, MongoDB, DynamoDB, RocksDB, memory; media on S3-compatible stores (Garage, AWS S3, R2, MinIO) or disk |
| Search | Typesense (opt-in per model, fields chosen by Admins) |
| Cache | Any server speaking the Redis protocol: Valkey (default), Redis, KeyDB, Dragonfly, managed services. Never tie code or docs to one vendor |
| `admin/` | Vike + React 19 + Tailwind 4 + daisyUI 5, linted with oxlint, formatted with Prettier |
| `website/` | Vike + React + Tailwind + daisyUI, Motion for animations, content managed in Tess, prerendered |
| `docs/` | Docusaurus 3, with local search (`@easyops-cn/docusaurus-search-local`) |
| `integrations/n8n-nodes-tess` | TypeScript n8n community nodes; n8n is embedded in the admin through the Tess proxy |
| Deployment | Docker (multi-arch amd64/arm64 images on GHCR), Docker Compose with profiles for optional services, AWS Lambda, Phusion Passenger, native binaries |
| CI | GitHub Actions: release builds on published releases, GitHub Pages for the website and docs |

Prefer the existing stack over new tools. Discuss before adding a framework,
a database, or a heavy dependency.

## Formatting and linting

- **Rust:** `cargo fmt --all` (rustfmt defaults, no config file), and
  `cargo clippy --workspace --all-targets` must be clean.
- **admin/ and website/:** Prettier (`prettier.config.js`) and oxlint
  (`npm run lint`). Run `npx prettier --write` on changed files.
- **Shell scripts:** bash with `set -euo pipefail`, clean under ShellCheck.
- **GitHub workflows:** clean under actionlint.
- **Dependencies:** pin exact versions in examples and scripts, and prefer
  releases that have been out for at least two weeks.

## Naming and URLs

- Website: <https://tessheadless.com>. Documentation:
  <https://tessheadless.com/docs/>. Never use `tessheadless.app`.
- **Inside `website/` and `docs/`, links between the two are relative** (`/docs/…`
  from the website, `/` from the docs), never `https://tessheadless.com/…`.
  Website links into `/docs/` get `rel="external"` (no Vike client routing);
  docs links to `/` are plain HTML (`html` navbar/footer items, `<a href="/">`
  in Markdown) so Docusaurus doesn't add its `/docs/` base.
- GitHub: <https://github.com/tessheadless/core>. Discord:
  <https://discord.gg/CyUjPVXK65>.
- Docker images: `ghcr.io/tessheadless/tess` and `ghcr.io/tessheadless/tess-n8n`,
  tagged `<version>`, `<version>-amd64`, `<version>-arm64`, `latest`.

## Versions and releases

- The app version is in [`VERSION`](VERSION) (`vX.Y.Z`). Change it only with
  `scripts/version.sh` (sets crates, admin, website, docs and the n8n nodes);
  `scripts/version.sh --check` must pass.
- Changelogs live on the GitHub releases page, not in a file
  ([CHANGELOG.md](CHANGELOG.md) explains the release steps).

## Configuration

- Settings are environment variables (`TESS_*`). A new one goes into
  `.env.example`, `docs/docs/devops/configuration.md`, and
  `scripts/init-env.sh` when users need to set it; optional services also get
  a Compose profile and a line in [INSTALL.md](INSTALL.md).
- Secrets are never printed, logged or committed.

## Documentation

- Docs are written for their audience: `getting-started`, `devops`,
  `developers`, `plugins`, `integrations`, `migration`.
- Code examples must work. Scripts shown in the docs live in `examples/` and
  are embedded with `{/* include: path */}`; run
  `scripts/sync-doc-examples.py` after changing them.
- `cd docs && npm run build` must pass (it fails on broken links).

## Testing

- `cargo test --workspace` for unit and API tests; `scripts/test-all.sh`
  also runs the conformance tests against real services in containers
  (databases, Garage, Typesense, Valkey).
- Tests that need a service read a `TESS_TEST_*` variable and skip when it's
  unset.
- Front-ends: `npm run build` and `npm run lint` in `admin/` and `website/`.
