// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const year = new Date().getFullYear();
const copyrightYears = year > 2026 ? `2026–${year}` : '2026';

// Docusaurus leaves links that already start with `baseUrl` unprefixed, so
// `/docs/intro` would break when the site itself lives under /docs/ (GitHub
// Pages build). There the doc pages are mounted at the site root instead.
const baseUrl = process.env.DOCS_BASE_URL || '/docs/';
const docsRoute = baseUrl === '/docs/' ? '' : '/docs';
const doc = (path: string) => `${docsRoute}/${path}`;

const config: Config = {
  title: 'Tess',
  tagline: 'The GraphQL-first headless CMS in Rust: every entry a tile, your products the mosaic.',
  favicon: 'img/logo.svg',

  future: {
    v4: true,
  },

  // Served at https://tessheadless.com/docs/, under the product website.
  // DOCS_URL / DOCS_BASE_URL override it (e.g. a fork's GitHub Pages origin).
  url: process.env.DOCS_URL || 'https://tessheadless.com',
  baseUrl,
  customFields: {docsRoute},

  onBrokenLinks: 'throw',
  markdown: {
    hooks: {onBrokenMarkdownLinks: 'throw'},
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          routeBasePath: docsRoute === '' ? '/' : 'docs',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  // Local search: the index is built with the site (`npm run build`), no
  // external service. It only works on a build (`npm run serve`), not in
  // `npm start`.
  themes: [
    [
      '@easyops-cn/docusaurus-search-local',
      {
        hashed: true,
        indexBlog: false,
        indexPages: false,
        // Must match the docs routeBasePath above.
        docsRouteBasePath: docsRoute === '' ? '/' : '/docs',
        language: ['en'],
        highlightSearchTermsOnTargetPage: true,
        explicitSearchResultPath: true,
        searchResultLimits: 10,
      },
    ],
  ],

  themeConfig: {
    colorMode: {
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'Tess',
      logo: {
        alt: 'Tess logo',
        src: 'img/logo.svg',
      },
      items: [
        {type: 'doc', docId: 'intro', position: 'left', label: 'Docs'},
        {to: doc('category/devops'), label: 'Deploy', position: 'left'},
        {to: doc('category/developers'), label: 'API', position: 'left'},
        {to: doc('category/plugins'), label: 'Plugins', position: 'left'},
        {to: doc('editions'), label: 'Community & Enterprise', position: 'right'},
        // The website, at the root of the same domain: plain HTML, so Docusaurus
        // doesn't treat `/` as one of its own routes (it would add /docs/).
        {type: 'html', position: 'right', value: '<a class="navbar__item navbar__link" href="/">tessheadless.com</a>'},
        {type: 'search', position: 'right'},
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Documentation',
          items: [
            {label: 'Introduction', to: doc('intro')},
            {label: 'Quick start', to: doc('getting-started/quick-start')},
            {label: 'Configuration reference', to: doc('devops/configuration')},
          ],
        },
        {
          title: 'Build',
          items: [
            {label: 'GraphQL API', to: doc('developers/graphql-api')},
            {label: 'Writing plugins', to: doc('plugins/overview')},
          ],
        },
        {
          title: 'Editions',
          items: [{label: 'Community & Enterprise', to: doc('editions')}],
        },
        {
          title: 'Community',
          items: [
            {html: '<a class="footer__link-item" href="/">tessheadless.com</a>'},
            {label: 'GitHub', href: 'https://github.com/tessheadless/core'},
            {label: 'Discord', href: 'https://discord.gg/CyUjPVXK65'},
          ],
        },
      ],
      copyright: `Copyright © ${copyrightYears} Tess Dev Team · <a href="/">tessheadless.com</a> · Community edition for personal use, Enterprise edition for commercial use.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['rust', 'toml', 'bash', 'docker', 'php', 'ruby', 'lua', 'graphql', 'yaml', 'json', 'go'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
