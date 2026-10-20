// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import "./tailwind.css";
import type { ReactNode } from "react";
import { usePageContext } from "vike-react/usePageContext";
import logoUrl from "../assets/logo.svg";
import { Markdown } from "../components/Markdown";
import { A, ButtonLink } from "../components/SiteLink";
import type { Settings } from "../lib/content";
import type { PageMeta } from "../lib/meta";

/** Used by pages without data (error pages). */
const FALLBACK: Settings = {
  siteName: "Tess",
  tagline: "",
  description: "",
  navigation: [],
  cta: null,
  footerLinks: [],
  footerText: null,
  docsUrl: "https://tessheadless.app/doc/",
};

export default function Layout({ children }: { children: ReactNode }) {
  const { data, urlPathname } = usePageContext();
  const settings = (data as PageMeta | undefined)?.settings ?? FALLBACK;
  const active = (url: string) => url !== "/" && (urlPathname === url || urlPathname.startsWith(`${url}/`));
  return (
    <div className="flex min-h-screen flex-col">
      <a href="#main" className="btn btn-sm absolute -top-20 left-2 z-50 focus:top-2">
        Skip to content
      </a>
      <header className="navbar bg-base-100/90 border-base-300 sticky top-0 z-40 border-b backdrop-blur">
        <div className="mx-auto flex w-full max-w-6xl items-center gap-2 px-2">
          <a href="/" className="flex items-center gap-2 px-2 text-xl font-bold">
            <img src={logoUrl} width={32} height={32} alt="" />
            {settings.siteName}
          </a>
          <nav aria-label="Main" className="ml-auto hidden md:block">
            <ul className="menu menu-horizontal gap-1">
              {settings.navigation.map((l) => (
                <li key={l.url}>
                  <A href={l.url} className={active(l.url) ? "menu-active" : undefined}>
                    {l.label}
                  </A>
                </li>
              ))}
            </ul>
          </nav>
          <ButtonLink link={settings.cta} className="btn-primary btn-sm ml-auto md:ml-2" />
          <details className="dropdown dropdown-end md:hidden">
            <summary className="btn btn-ghost btn-square" aria-label="Open navigation">
              <svg
                viewBox="0 0 24 24"
                className="size-6"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                aria-hidden="true"
              >
                <path strokeLinecap="round" d="M4 6h16M4 12h16M4 18h16" />
              </svg>
            </summary>
            <ul className="menu dropdown-content bg-base-200 rounded-box z-50 mt-2 w-52 p-2 shadow">
              {settings.navigation.map((l) => (
                <li key={l.url}>
                  <A href={l.url}>{l.label}</A>
                </li>
              ))}
            </ul>
          </details>
        </div>
      </header>
      <main id="main" className="flex-1">
        {children}
      </main>
      <footer className="bg-base-200 mt-16">
        <div className="mx-auto grid w-full max-w-6xl gap-8 px-4 py-12 md:grid-cols-[2fr_1fr]">
          <div>
            <div className="flex items-center gap-2 text-lg font-bold">
              <img src={logoUrl} width={28} height={28} alt="" />
              {settings.siteName}
            </div>
            <p className="text-base-content/70 mt-2">{settings.tagline}</p>
            <Markdown className="prose-sm text-base-content/60 mt-4">{settings.footerText}</Markdown>
          </div>
          <nav aria-label="Footer">
            <ul className="space-y-2">
              {settings.footerLinks.map((l) => (
                <li key={l.url}>
                  <A href={l.url} className="link link-hover">
                    {l.label}
                  </A>
                </li>
              ))}
            </ul>
          </nav>
        </div>
      </footer>
    </div>
  );
}
