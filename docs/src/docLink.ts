// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

/** Route of a doc page (`intro` → `/docs/intro`), wherever the docs are mounted (see docusaurus.config.ts). */
export function useDocLink(): (path: string) => string {
  const {siteConfig} = useDocusaurusContext();
  const route = (siteConfig.customFields?.docsRoute as string | undefined) ?? '/docs';
  return (path) => `${route}/${path}`;
}
