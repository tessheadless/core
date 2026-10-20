// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { ReactNode } from "react";
import { usePageContext } from "vike-react/usePageContext";

/** Prefix an app path ("/plugins") with the deployment base ("/admin/"). */
export function withBase(path: string): string {
  const base = import.meta.env.BASE_URL.replace(/\/$/, "");
  return `${base}${path}`;
}

interface NavLinkProps {
  /** Path without the base, e.g. "/plugins". */
  href: string;
  children: ReactNode;
  onClick?: () => void;
}

/** Sidebar menu link, highlighted when its section is active. */
export function NavLink({ href, children, onClick }: NavLinkProps) {
  // urlPathname excludes the base URL.
  const { urlPathname } = usePageContext();
  // Whole segments only: "/content" must not match "/content-models".
  const isActive = href === "/" ? urlPathname === "/" : urlPathname === href || urlPathname.startsWith(`${href}/`);
  return (
    <a
      href={withBase(href)}
      className={isActive ? "menu-active" : undefined}
      aria-current={isActive ? "page" : undefined}
      onClick={onClick}
    >
      {children}
    </a>
  );
}
