# Tess documentation site

Built with [Docusaurus](https://docusaurus.io/).

```bash
npm install
npm start        # local dev server with live reload
npm run build    # static site in build/ (fails on broken links)
npm run serve    # preview the production build
```

Content lives in `docs/`, one folder per audience: `getting-started`,
`devops`, `developers` and `plugins`. The sidebar is generated from the
folders (`_category_.json`) and from each page's `sidebar_position`.
