# Next.js (App Router)

- `lib/cms.ts` (server-only): `fetch` with `next: { revalidate: 60, tags }`, typed errors carrying `extensions.code`.
- `app/page.tsx`: post list (ISR, 60 s). `app/posts/[slug]/page.tsx`: post by slug, `generateStaticParams`
  prerenders known slugs, new ones render on demand, unknown ones `notFound()`.
- `app/search/page.tsx`: search page (`?q=`) using `searchPosts`.
- `app/api/search/route.ts`: `GET /api/search?q=...` JSON route handler for client-side widgets.

`TESS_URL` is a server-side env var (not `NEXT_PUBLIC_`): the browser never talks to the CMS.

```sh
npm install
TESS_URL=http://localhost:8080 npm run build   # the build fetches from the CMS
TESS_URL=http://localhost:8080 npm start
```
