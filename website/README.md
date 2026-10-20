# Tess website

The marketing website of Tess ([tessheadless.com](https://tessheadless.com)), built with the same
stack as the admin UI (Vike, React, Tailwind CSS, daisyUI). Its content lives **in Tess**: edit it in the
admin, then rebuild.

Every page is prerendered to static HTML at build time; `dist/client/` can be deployed to any static host
(S3/CloudFront, Cloudflare Pages, Netlify, Nginx…). Nothing calls Tess from the browser.

## Content

| Model            | Used for                                                                          |
| ---------------- | --------------------------------------------------------------------------------- |
| `SiteSettings`   | Site name, tagline, navigation, header button, footer. Keep one published entry.  |
| `SitePage`       | Pages at `/{slug}` (`home` is `/`), made of sections (a dynamic zone).            |
| `SiteFeature`    | Cards of the feature grids, sorted by `order`.                                    |
| `SiteCompetitor` | `/compare` and `/compare/{slug}`, including the link to the import & export docs. |

Page sections are components: `SiteHero`, `SiteFeatureGrid`, `SiteRichText`, `SiteCodeSample`,
`SiteComparisonTeaser`, `SiteFaq` and `SiteCallToAction`. A new page only needs a new `SitePage` entry; a new
section type also needs a case in `components/Sections.tsx`.

Models are defined in [`content/models.yaml`](content/models.yaml), in the `tess-transfer` format. The initial
content is [`content/seed.json`](content/seed.json).

## Set up a Tess instance

```sh
export TESS_URL=https://cms.example.com TESS_TOKEN=…   # Admin token
tess-transfer import content/models.yaml               # add --update after changing the file
TESS_API_URL=$TESS_URL npm run seed                     # creates and publishes the content
```

`npm run seed` never overwrites existing entries (pass `-- --overwrite` to reset them to the seed), so it is
safe to run again.

## Build

```sh
npm install
TESS_API_URL=https://cms.example.com npm run build     # reads PUBLISHED content
npm run preview                                         # serve the build locally
```

- `TESS_API_URL`: the Tess instance. Without it, the site is built from `content/seed.json` (handy offline),
  and the build logs a warning.
- `TESS_TOKEN`: only needed if the instance disables public reads (`TESS_PUBLIC_READ=false`); a Member token
  is enough.

If Tess is unreachable or returns errors, the build fails instead of publishing an empty site.

## Publishing changes

Content changes appear after the next build. Trigger it automatically by calling your host's build hook (or
CI, e.g. a GitHub Actions `repository_dispatch`) when a `Site*` entry is published, unpublished or deleted.

## Comparisons

Comparison facts are dated (`asOf`) and come from each vendor's public documentation. Review them when you
update an entry, and update `asOf` with them.
