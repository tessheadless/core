# [Tess](https://tessheadless.com/) Changelog

The changes of each Tess version are published on the
**[GitHub releases page](https://github.com/tessheadless/core/releases)**, not in
this file.

Each release there lists:

- **What changed:** new features, fixes and breaking changes, with links to the
  pull requests.
- **Upgrade notes:** anything to do before or after upgrading (settings,
  migrations, deprecations). Read them before you upgrade.
- **Downloads:** binaries for Linux, macOS and Windows, the Docker Compose
  bundles and `SHA256SUMS`. The Docker images are
  `ghcr.io/tessheadless/tess:<version>` and `ghcr.io/tessheadless/tess-n8n:<version>`
  (see [INSTALL.md](INSTALL.md)).

Versions follow [Semantic Versioning](https://semver.org): `vMAJOR.MINOR.PATCH`.
Until 1.0, a minor version may contain breaking changes; they are always listed
in its upgrade notes. The current version is in [`VERSION`](VERSION).

## For maintainers

1. Set the version in every sub-project and commit:

   ```sh
   scripts/version.sh minor        # or patch, major, or an explicit v1.4.0
   git commit -am "Release $(scripts/version.sh)"
   ```

2. Tag the commit with the same version and push:

   ```sh
   git tag "$(scripts/version.sh)" && git push --follow-tags
   ```

3. Create the release from the tag on GitHub, with **Generate release notes**,
   and complete the upgrade notes. Publishing it starts the release workflow,
   which builds and attaches everything above. It fails if the tag doesn't
   match `VERSION`.
