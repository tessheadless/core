# Tess documentation site

Built with [Docusaurus](https://docusaurus.io/).

```bash
npm install
npm start        # local dev server with live reload
npm run build    # static site in build/ (fails on broken links)
npm run serve    # preview the production build
```

Content lives in `docs/`, one folder per audience: `getting-started`,
`devops`, `developers`, `plugins`, `integrations` and `migration`. The sidebar is generated from the
folders (`_category_.json`) and from each page's `sidebar_position`.

## Search

The search box in the header is local
([`@easyops-cn/docusaurus-search-local`](https://github.com/easyops-cn/docusaurus-search-local)):
the index is built with the site, so it works with `npm run build` and
`npm run serve` (and on GitHub Pages), but not with `npm start`.

## Embedded examples

Pages can show a file from the repository with an MDX comment followed by a
code block, e.g. `{/* include: examples/migrate/strapi.mjs */}`. Run
`scripts/sync-doc-examples.py` after changing the file (`--check` in
`scripts/test-all.sh` fails when a page is out of date).
