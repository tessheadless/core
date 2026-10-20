---
sidebar_position: 10
title: Import & export
---

# Importing and exporting models and users

`tess-transfer` copies **content models** and **users** between Tess
instances: from development to production, into a fresh install, or into
version control (models only). It talks to the GraphQL API, so it works with
every deployment (Docker, Lambda…) and every database. It's shipped in the
Docker image next to `tess`, or you can build it with
`cargo build --release -p tess-transfer`.

```bash
export TESS_URL=https://cms.example.com
export TESS_TOKEN=…        # Admin role: session token, API token or bootstrap token

tess-transfer export -o tess.yaml                     # models + users
tess-transfer export --only models -o models.json     # JSON, models only
tess-transfer import tess.yaml --dry-run              # show the plan
tess-transfer import tess.yaml --passwords-out new-users.txt
tess-transfer import models.yaml --update             # also replace existing models
```

The token is read from `TESS_TOKEN` only, never from the command line, so it
doesn't end up in your shell history. Importing Owners requires an Owner
token.

## File format

YAML or JSON, chosen from the file extension (`.yaml`, `.yml`, `.json`) or
with `--format yaml|json`. Defaults are omitted.

```yaml
version: 1
components:
  - apiId: Seo
    fields:
      - {name: metaTitle, type: TEXT, maxLength: 60}
      - {name: metaDescription, type: TEXT, maxLength: null}   # null = unlimited
contentTypes:
  - apiId: Post
    pluralApiId: posts           # optional
    description: Blog posts      # optional
    workflow: true               # optional (default false)
    fields:
      - {name: title, type: TEXT, required: true, maxLength: 120}
      - {name: status, type: ENUM, enumValues: [DRAFTED, LIVE]}
      - {name: cover, type: RELATION, relationTarget: Asset}
      - {name: tags, type: RELATION, relationTarget: Tag, many: true}
      - {name: seo, type: COMPONENT, component: Seo}
  - apiId: Tag
    fields:
      - {name: label, type: TEXT}
users:
  - email: ada@example.com
    name: Ada Example
    role: ADMIN                  # OWNER, ADMIN, CONTRIBUTOR, REVIEWER, MEMBER
    scope: [Post]                # optional, for Contributors and Reviewers
    disabled: false              # optional
    # password: …                # optional, import only; never exported
```

Unknown keys are rejected, with their line and column.

## How imports behave

- **Validation first.** The whole file is checked against the server before
  anything changes: duplicates, relations to unknown content types, missing
  emails or roles, and the passwords file.
- **Models in any order.** Components are created before content types,
  nested components before the components containing them. Models can
  reference each other in cycles (`Post` ↔ `Tag`): relations to models that
  don't exist yet are added in a second pass.
- **`maxLength` of TEXT fields**: absent means the server default (80),
  `null` means unlimited. Exports always write it.
- **Existing models and users are skipped**, unless you pass `--update`:
  - **Models:** their definition is replaced, exactly like an edit in the
    admin.
  - **Users:** their name, role, scope and status are updated. Passwords are
    never changed.
- **The built-in `Asset` type** is never exported or imported.
- **Passwords:** exports never contain them, since only hashes exist on the
  server. New users without a `password` get a random one, written to the
  `--passwords-out` file (mode 600) and never printed. Share each password
  privately, ask users to change it, and delete the file.
- **Errors:** each failure is reported, and the exit code is non-zero when
  anything failed.

:::warning Personal data
A file with users contains staff email addresses. Keep it out of version
control and delete it after use. Model-only exports (`--only models`) are
safe to commit.
:::

## In Docker

```bash
docker compose exec -e TESS_TOKEN tess tess-transfer export --only models > models.yaml
docker compose exec -T -e TESS_TOKEN tess tess-transfer import - --format yaml < models.yaml
```

(`TESS_URL` defaults to `http://localhost:8080`, which is the server itself
inside the container.)
