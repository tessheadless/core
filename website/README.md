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
content is [`content/seed.json`](content/seed.json) (English) and `content/seed.{fr,zh,es}.json`.

## Languages

The site is in English (`/`), French (`/fr/`), Simplified Chinese (`/zh/`) and Spanish (`/es/`). Every page is
prerendered once per language, and a menu in the header switches between them. The documentation stays in
English.

- **Content:** every `Site*` model has a `locale` field (`EN`, `FR`, `ZH`, `ES`; empty means English). Translate an
  entry by creating another one with the same `slug` (pages, competitors) and the other locale; for features and
  settings, create the entries of that language. Where a language has no entry, the site shows the English one.
- **Interface strings** (buttons, table headers, the 404 page…) are i18next resources in
  `locales/{locale}/translation.json`, bundled with the site.
- Links in the content stay unprefixed (`/compare`): the site adds the language (`/fr/compare`). Links to `/docs/`
  are left as they are.
- **Adding a language:** add it to `LOCALES` and `LOCALE_INFO` in [`lib/i18n.ts`](lib/i18n.ts), to the `locale`
  enum in `content/models.yaml`, to the seed script, and create its `locales/` file and seed file.

## Set up a Tess instance

```sh
export TESS_URL=https://cms.example.com TESS_TOKEN=…   # Admin token
tess-transfer import content/models.yaml               # add --update after changing the file
TESS_API_URL=$TESS_URL npm run seed                     # creates and publishes the content
```

`npm run seed` loads every language; `-- --locale fr` loads one. It never overwrites existing entries (pass
`-- --overwrite` to reset them to the seed), so it is safe to run again. Entries created before the `locale` field
existed count as English.

After upgrading from a version without languages, import the models with `--update` (adds the `locale` field),
then run `npm run seed` to add the translations.

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
- `SITE_URL`: the public origin (`https://www.tessheadless.com`). With it, pages get canonical and `hreflang`
  links to their translations (search engines need absolute URLs).

If Tess is unreachable or returns errors, the build fails instead of publishing an empty site.

## Publishing changes

Content changes appear after the next build. Trigger it automatically by calling your host's build hook (or
CI, e.g. a GitHub Actions `repository_dispatch`) when a `Site*` entry is published, unpublished or deleted.

## Comparisons

Comparison facts are dated (`asOf`) and come from each vendor's public documentation. Review them when you
update an entry, and update `asOf` with them.
