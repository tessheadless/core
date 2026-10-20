# Vike + React (prerendered)

Static blog prerendered at build time:

- `pages/index/+data.ts` fetches the post list, `pages/posts/@slug/+data.ts` fetches one post by slug
  (`render(404)` when missing).
- `pages/posts/@slug/+onBeforePrerenderStart.ts` tells Vike which `/posts/<slug>` URLs exist.
- `lib/cms.ts` is the GraphQL helper; list keys use `ref`, links use `slug`.

`TESS_URL` is read at build time only (the browser never talks to the CMS).

```sh
npm install
TESS_URL=http://localhost:8080 npm run build   # writes dist/client/**.html
npm run preview
```

Rebuild to pick up new content.
