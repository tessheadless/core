# n8n-nodes-tess

n8n nodes for [Tess](https://tessheadless.com):

- **Tess Trigger**: starts a workflow on Tess events (`entry.publish`, `entry.*`,
  `contentType.update`, `i18n.translation.update`, …). Activating the workflow
  registers a signed webhook in Tess (shown under *Admin → Webhooks*, managed by
  `n8n`); deactivating removes it. Signatures (`X-Tess-Signature`) are verified.
- **Tess**: entries (get, list, create, update, delete, publish, unpublish),
  translations (list keys, set texts and review states, export, publish to
  delivery targets) and raw GraphQL.
- **Tess API** credential: the Tess URL as n8n reaches it and an API token
  (Admin role for the trigger).

## Install

The `n8n` profile of the Tess Docker Compose file builds an n8n image with these
nodes (`deploy/n8n/Dockerfile`). Elsewhere:

```sh
npm ci && npm run build
# copy (or npm link) the package into ~/.n8n/custom/, or set N8N_CUSTOM_EXTENSIONS
```

When Tess reaches n8n by another address than the public one (Docker
networks), fill **n8n URL for Tess** on the trigger, e.g. `http://n8n:5678`.

## Develop

```sh
npm test   # builds, then runs the unit tests
```
