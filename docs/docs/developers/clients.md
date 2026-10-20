---
sidebar_position: 10
title: Client examples
---

# Client examples

The repository's `examples/clients/` folder has small, self-contained clients
in ten stacks. They all use the demo model created by `scripts/seed.sh`
(`Author { name }` and `Post { title, slug, excerpt, body, tags, author }`).

| Folder | Stack | Shows |
|---|---|---|
| `vike` | Vike + React | prerendered list and `/posts/@slug` pages |
| `gatsby` | Gatsby 5 | `sourceNodes` from `posts`, a page per slug |
| `nextjs` | Next.js App Router | server components with `revalidate`, `/posts/[slug]`, search page and route handler |
| `nuxt` | Nuxt 4 | `useAsyncData`, server API routes, prerendering |
| `python` | `requests` | list, search, by slug, error codes, create + publish, media upload |
| `ruby` | stdlib | list, search, by slug, error codes, create + publish |
| `go` | stdlib | typed client, all of the above and media upload |
| `rust` | `reqwest` + `serde` + `tokio` | typed structs, all of the above and media upload |
| `php` | PHP 8 + curl | all of the above and media upload |
| `lua` | `luasocket` + `dkjson` | all of the above and media upload |

## Run them

```bash
scripts/dev-server.sh &                          # throwaway server on :8080
source "${XDG_RUNTIME_DIR:-/tmp}/tess-dev.env"    # exports TESS_ADMIN_TOKEN
TESS_URL=http://localhost:8080 scripts/seed.sh
```

Each example reads `TESS_URL`, plus `TESS_ADMIN_TOKEN` for its optional admin
part. In your own projects, use an [API token](authentication.md#api-tokens-admins)
with the smallest role that works, and never ship a token to a browser.

## Recommendations

- **Static sites and ISR**: fetch published content at build or revalidation
  time; anonymous reads need no token. Use `ref` for keys and `slug` for
  routes ([why](ids-and-refs.md)).
- **Previews**: query `stage: DRAFT` from your server with a `MEMBER` token.
- **Errors**: branch on `extensions.code` ([list](errors.md)).
