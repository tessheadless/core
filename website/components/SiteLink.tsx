// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { ReactNode } from "react";
import type { SiteLink as Link } from "../lib/content";
import { isExternal, isOtherApp } from "../lib/meta";

/** A button-styled link; `link` may be null (optional component field). */
export function ButtonLink({ link, className }: { link: Link | null | undefined; className: string }) {
  if (!link) return null;
  return (
    <A href={link.url} className={`btn ${className}`}>
      {link.label}
    </A>
  );
}

/** `rel` of a link: other sites, and the docs (no client-side navigation). */
export function linkRel(href: string): string | undefined {
  if (isExternal(href)) return "noopener";
  if (isOtherApp(href)) return "external";
  return undefined;
}

export function A({ href, className, children }: { href: string; className?: string; children: ReactNode }) {
  return (
    <a href={href} className={className} rel={linkRel(href)}>
      {children}
    </a>
  );
}
