# Nuxt 4

- `nuxt.config.ts`: private `runtimeConfig.cmsUrl` (defaults to `TESS_URL` at build time, overridable at runtime
  with `NUXT_CMS_URL`); Nitro prerenders `/` and crawls to every `/posts/<slug>`.
- `server/utils/cms.ts` + `server/api/posts.get.ts` (list, or `?q=` search via `searchPosts`) and
  `server/api/posts/[slug].get.ts` (by slug, 404 when missing): the browser only talks to these routes.
- `app/pages/index.vue` and `app/pages/posts/[slug].vue` use `useAsyncData` + `$fetch`; list keys use `ref`.

```sh
npm install
TESS_URL=http://localhost:8080 npm run build   # prerenders pages from the CMS
NUXT_CMS_URL=http://localhost:8080 node .output/server/index.mjs
```
