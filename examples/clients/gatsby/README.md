# Gatsby

- `gatsby-node.ts` → `sourceNodes` pages through `posts(first, skip)` and creates `CmsPost` nodes
  (node id derived from the stable `ref`), `createSchemaCustomization` declares the type, and
  `createPages` makes one page per slug from `src/templates/post.tsx`.
- `src/pages/index.tsx` lists the posts with a page query.

`TESS_URL` is read at build time.

```sh
npm install
TESS_URL=http://localhost:8080 npm run build   # writes public/
npm run serve
```
