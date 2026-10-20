---
sidebar_position: 8
title: Security
---

# Security checklist

## Before going to production

- [ ] **Secrets**: `TESS_ID_SECRET`, `TESS_REF_SECRET` and
      `TESS_SESSION_SECRET` are random (`openssl rand -hex 32`), different
      from each other, and stored in a secrets manager.
- [ ] **Bootstrap token**: the first Owner exists and `TESS_ADMIN_TOKEN` is
      **removed**. Tess logs a warning while it's set.
- [ ] **TLS**: a reverse proxy or load balancer terminates TLS (Tess speaks
      plain HTTP).
- [ ] **CORS**: `TESS_CORS_ORIGINS` lists your front-end and admin origins
      instead of `*`.
- [ ] **Public read**: decide whether anonymous clients may read published
      content (`TESS_PUBLIC_READ`).
- [ ] **GraphiQL**: `TESS_GRAPHIQL=false` in production, if you don't want the
      IDE exposed. Introspection (`TESS_INTROSPECTION`) is used by the admin
      UI's review queue.
- [ ] **Limits**: `TESS_MAX_PAGE_SIZE` and `TESS_MAX_QUERY_DEPTH` fit your
      use.
- [ ] **Plugin network access**: `TESS_PLUGIN_ALLOW_HTTP=false` if plugins
      never need it.
- [ ] **Backups** of the database *and* the media store (see
      [Operations](operations.md)).

## Authentication model

| Credential | Format | Lifetime | Revocation |
|---|---|---|---|
| User session | `tess_s1.…` | `TESS_SESSION_TTL` (12 h) | immediate: role/scope/status/password change, "sign out everywhere", deleting the user, or rotating `TESS_SESSION_SECRET` (everyone) |
| API token | `tess_t1_…` | optional expiry | revoke in the admin or with `revokeApiToken` |
| Bootstrap token | `TESS_ADMIN_TOKEN` | while configured | remove it from the configuration |

- **Passwords** are hashed with Argon2id and need at least 12 characters.
- **Lockout**: after 10 failed sign-ins, an account is locked for 15
  minutes.
- **Login errors** don't reveal whether an account exists.
- **Sessions are stateless and signed.** Every request reloads the user, so
  revocation takes effect immediately on every instance, Lambda included.
- **API token secrets** are shown once; only a SHA-256 hash is stored.

Roles and what they allow are described in
[Authentication](../developers/authentication.md).

## Opaque ids

Entry ids in the API are encrypted (XChaCha20-Poly1305), type-bound, and
expire after `TESS_ID_TTL`. Database ids never leave the server, not even to
plugins.

### Rotating the id secret

1. Move the current secret to `TESS_ID_SECRET_PREVIOUS` and set a new
   `TESS_ID_SECRET`.
2. Old ids keep working until they expire.
3. After `TESS_ID_TTL`, remove the previous secret.

`TESS_REF_SECRET` should not be rotated casually: every `ref` changes, and
clients that cache by `ref` lose their keys.

## Plugins

Plugins are WebAssembly modules running in a sandbox with:

- **No access** to the filesystem or environment variables.
- **Limits**: a timeout and a memory cap.
- **Network**: none unless an admin lists allowed hosts, and never if
  `TESS_PLUGIN_ALLOW_HTTP=false`.
- **Integrity**: a SHA-256 checksum, verified when the module is loaded.

Only Admins and Owners can install plugins, and they're installed disabled.
Treat third-party modules like any dependency: review their source and their
requested hosts. See [Sandbox](../plugins/sandbox.md).

## Media

Files served by Tess (filesystem store) carry `nosniff` and a sandboxing
Content-Security-Policy. For S3 stores, keep the bucket private, and serve
files through presigned URLs or a CDN.

## Known limitations

- **Email uniqueness isn't enforced atomically.** Two Admins creating the
  same email address at the same instant could both succeed.
- **No built-in rate limiting** beyond the per-account lockout. Put a rate
  limiter in front of `/graphql` (reverse proxy or API gateway).
- **Single sign-on (OIDC)** is planned for the [Enterprise edition](../editions.md).
