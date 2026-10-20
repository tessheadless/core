# Migration examples

Scripts that copy media and entries from another CMS to Tess, with the Tess
models of their example projects. Each one is explained step by step in the
docs, which embed them (`scripts/sync-doc-examples.py` keeps the pages up to
date):

| Source | Script | Models | Guide |
|---|---|---|---|
| Strapi 5 (REST API) | [`strapi.mjs`](strapi.mjs) | [`strapi-models.yaml`](strapi-models.yaml) | [Migrate from Strapi](https://tessheadless.com/docs/migration/strapi) |
| Hygraph (Content API) | [`hygraph.mjs`](hygraph.mjs) | [`hygraph-models.yaml`](hygraph-models.yaml) | [Migrate from Hygraph](https://tessheadless.com/docs/migration/hygraph) |
| WordPress without plugins (REST API) | [`wordpress.mjs`](wordpress.mjs) | [`wordpress-models.yaml`](wordpress-models.yaml) | [Migrate from WordPress](https://tessheadless.com/docs/migration/wordpress) |

Node.js 20+. Only `wordpress.mjs` has dependencies (`npm install`, to convert
HTML to Markdown). The source CMS is only read.
