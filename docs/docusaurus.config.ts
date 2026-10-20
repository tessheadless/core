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
const baseUrl = process.env.DOCS_BASE_URL || '/doc/';
const docsRoute = baseUrl === '/docs/' ? '' : '/docs';
const doc = (path: string) => `${docsRoute}/${path}`;

const config: Config = {
  title: 'Tess',
  tagline: 'The GraphQL-first headless CMS in Rust: every entry a tile, your products the mosaic.',
  favicon: 'img/logo.svg',

  future: {
    v4: true,
  },

  // Served at https://tessheadless.app/doc/ (product website: https://tessheadless.com).
  // The GitHub Pages workflow overrides both to serve the docs under /docs/ of the site.
  url: process.env.DOCS_URL || 'https://tessheadless.app',
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
        {href: 'https://tessheadless.com', label: 'tessheadless.com', position: 'right'},
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
            {label: 'tessheadless.com', href: 'https://tessheadless.com'},
            {label: 'GitHub', href: 'https://github.com/tessheadless/core'},
            {label: 'Discord', href: 'https://discord.gg/CyUjPVXK65'},
          ],
        },
      ],
      copyright: `Copyright © ${copyrightYears} Tess Dev Team · <a href="https://tessheadless.com">tessheadless.com</a> · Community edition for personal use, Enterprise edition for commercial use.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['rust', 'toml', 'bash', 'docker', 'php', 'ruby', 'lua', 'graphql', 'yaml', 'json', 'go'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
