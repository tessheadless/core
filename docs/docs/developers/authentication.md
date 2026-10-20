---
sidebar_position: 4
title: Authentication, roles and API tokens
---

# Authentication, roles and API tokens

Requests without credentials are **public**: they can read `PUBLISHED`
content, unless the operator sets `TESS_PUBLIC_READ=false`. Everything else
needs `Authorization: Bearer <token>`.

| Token | Looks like | Obtained with |
|---|---|---|
| User session | `tess_s1.…` | `login` |
| API token | `tess_t1_…` | `createApiToken` (Admin) |
| Bootstrap token | operator-defined | `TESS_ADMIN_TOKEN`, only to create the first Owner |

Invalid, expired or revoked credentials get **HTTP 401**. A valid credential
without the required permission gets a GraphQL error with code `FORBIDDEN`.

## Signing in

```graphql
mutation {
  login(email: "ada@example.com", password: "…") {
    token          # send as: Authorization: Bearer <token>
    expiresAt
    user { name role scope }
  }
}
```

| Operation | |
|---|---|
| `setupRequired: Boolean!` | `true` until the first user exists (public) |
| `me: User` | the signed-in user (`null` for API and bootstrap tokens) |
| `changePassword(currentPassword, newPassword): Session!` | returns a **new** token; every other session is revoked |
| `signOutEverywhere: Boolean!` | revokes all of your sessions, this one included |

Sessions last `TESS_SESSION_TTL` (12 h by default); sign in again afterwards.
Passwords need at least 12 characters. After 10 failed attempts, an account
is locked for 15 minutes.

## Roles

| | Owner | Admin | Contributor | Reviewer | Member |
|---|:-:|:-:|:-:|:-:|:-:|
| Manage Owners | ✔ | | | | |
| Manage users, API tokens, content models, plugins, reindex search | ✔ | ✔ | | | |
| Publish anything, bypass review | ✔ | ✔ | | | |
| Approve / request changes (workflow types) | ✔ | ✔ | | ✔ | |
| Create and edit drafts, submit for review, upload media | ✔ | ✔ | ✔ | | |
| Publish and unpublish types **without** workflow | ✔ | ✔ | ✔ | | |
| Delete entries | ✔ | ✔ | never-published only | | |
| Read drafts and media | ✔ | ✔ | ✔ | ✔ | ✔ |

**Scope**: a Contributor or Reviewer can be limited to a list of content
types (`scope`). An empty list means every type. Admins and Owners always
have access to all types.

## Managing users (Admins)

```graphql
mutation {
  createUser(input: {
    email: "carl@example.com", name: "Carl Example",
    role: CONTRIBUTOR, scope: ["Post", "Page"],
    password: "a long initial password"
  }) { id role scope }
}
```

| Operation | |
|---|---|
| `users: [User!]!` | list users |
| `updateUser(id, input: { name, role, scope, disabled, password })` | omitted fields are unchanged |
| `deleteUser(id)` | delete |

Rules:

- **Owners**: only Owners can create, modify or delete them, and there is
  always at least one active Owner.
- **Yourself**: nobody can change their own role, or disable or delete their
  own account.
- **Sign-out**: changing a user's role, scope, status or password signs them
  out everywhere.

## API tokens (Admins)

For CI, static-site builds, webhooks and back-ends:

```graphql
mutation {
  createApiToken(name: "Website build", role: MEMBER, expiresInDays: 90) {
    token                      # shown once: store it in your secrets manager
    apiToken { id name role expiresAt }
  }
}
```

A token acts with its role and scope. Admins can't create Owner tokens.
List tokens with `apiTokens`, and revoke them with `revokeApiToken(id)`.
For a front-end that only reads drafts (previews), a `MEMBER` token is
enough.

:::tip Single sign-on
Sign-in with your identity provider (OIDC) is planned for the
[Enterprise edition](../editions.md).
:::
