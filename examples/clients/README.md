# Client examples

Small, self-contained clients for the CMS GraphQL API, one folder per stack.
All of them use the demo content model created by `scripts/seed.sh`:
`Author { name }` and `Post { title, slug, excerpt, body, tags (JSON), author -> Author }`.

| Folder | Stack | Shows |
|---|---|---|
| [`vike`](vike) | Vike + React, Vite | Prerendered list + `/posts/@slug` (`+data.ts`, `+onBeforePrerenderStart.ts`) |
| [`gatsby`](gatsby) | Gatsby 5 | `sourceNodes` from `posts` (paginated), list page, page per slug |
| [`nextjs`](nextjs) | Next.js App Router | Server components with `next: { revalidate: 60 }`, `/posts/[slug]`, `/search` page + `/api/search` route handler (`searchPosts`) |
| [`nuxt`](nuxt) | Nuxt 4 | `useAsyncData` + `$fetch`, server API routes, `runtimeConfig`, list + search + `/posts/[slug]`, prerendered at build |
| [`python`](python) | `requests` | list, search, by slug, error codes, admin create + publish, media upload |
| [`ruby`](ruby) | stdlib (`net/http`, `json`) | list, search, by slug, error codes, admin create + publish |
| [`go`](go) | stdlib only | typed client, list, search, by slug, error codes, admin create + publish, media upload |
| [`rust`](rust) | `reqwest` (rustls) + `serde` + `tokio` | typed structs, same features as Go incl. media upload |
| [`php`](php) | PHP 8 + ext-curl | same features incl. media upload |
| [`lua`](lua) | `luasocket` + `dkjson` | same features incl. media upload |

## Try them

```sh
scripts/dev-server.sh &                        # throwaway in-memory server on :8080
source "${XDG_RUNTIME_DIR:-/tmp}/tess-dev.env"  # exports TESS_ADMIN_TOKEN (never commit it)
TESS_URL=http://localhost:8080 scripts/seed.sh
```

Every example reads:

- `TESS_URL` – base URL of the server (default `http://localhost:8080`); the GraphQL endpoint is `POST $TESS_URL/graphql`.
- `TESS_ADMIN_TOKEN` – optional, only for the admin part of the backend examples (create/publish, media upload).
  It is read from the environment only; never hard-code it and never ship it to a browser.
  Without it the admin part is skipped.

Anonymous requests only see **published** content. The front-end examples never send the token.

## IDs vs refs vs slugs

- `id` is **opaque, encrypted and expiring** (24 h by default). Use it only for an immediate follow-up
  call (e.g. `publishPost(id:)` right after `createPost`). Never store it, put it in a URL, or use it as a cache key.
  An old id fails with `ID_EXPIRED`.
- `ref` is a **stable opaque reference**: use it for React/Vue list keys, cache keys, Gatsby node ids.
- `slug` is what you route on: `posts(where: { slug: $slug }, first: 1)`. The CMS does not enforce slug
  uniqueness, so the static-site examples de-duplicate slugs.

## Errors

Errors come back as GraphQL `errors[]` (HTTP 200) with `extensions.code`, e.g. `FORBIDDEN`
(write without a token), `BAD_USER_INPUT` (malformed id, invalid data), `NOT_FOUND`, `ID_EXPIRED`.
Each example turns them into a typed error carrying the code. REST endpoints (`/media/upload`)
answer with an HTTP error status and `{ "error", "code" }`.

## Media upload

`POST $TESS_URL/media/upload?fileName=x.png` with the raw bytes as body, the MIME type as `Content-Type`
and `Authorization: Bearer $TESS_ADMIN_TOKEN` returns `{ id, ref, fileName, mimeType, size, url, ... }`.
With the filesystem store `url` is server-relative (`/media/files/...`): prefix it with `TESS_URL`.
