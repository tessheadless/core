// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import "./tailwind.css";
import { MotionConfig } from "motion/react";
import type { ReactNode } from "react";
import { I18nextProvider, useTranslation } from "react-i18next";
import { usePageContext } from "vike-react/usePageContext";
import logoUrl from "../assets/logo.svg";
import { Markdown } from "../components/Markdown";
import { PageFade } from "../components/Motion";
import { A, ButtonLink } from "../components/SiteLink";
import type { Settings } from "../lib/content";
import { i18nFor, LOCALE_INFO, LOCALES, localizeUrl } from "../lib/i18n";
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
  docsUrl: "/docs/",
};

export default function Layout({ children }: { children: ReactNode }) {
  const { locale } = usePageContext();
  return (
    <I18nextProvider i18n={i18nFor(locale)}>
      <Frame>{children}</Frame>
    </I18nextProvider>
  );
}

/** Links to the current page in every language. */
function LanguageMenu({ className = "" }: { className?: string }) {
  const { locale, urlPathname } = usePageContext();
  const { t } = useTranslation();
  return (
    // Keyed by page and language: a new page starts with the menu closed.
    <details key={`${locale}${urlPathname}`} className={`dropdown dropdown-end ${className}`}>
      <summary className="btn btn-ghost btn-sm gap-1" aria-label={t("layout.language")}>
        <svg
          viewBox="0 0 24 24"
          className="size-5"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.8"
          aria-hidden="true"
        >
          <circle cx="12" cy="12" r="9" />
          <path d="M3 12h18M12 3c2.5 2.7 3.8 5.7 3.8 9s-1.3 6.3-3.8 9c-2.5-2.7-3.8-5.7-3.8-9S9.5 5.7 12 3z" />
        </svg>
        <span className="uppercase">{locale}</span>
      </summary>
      <ul className="menu dropdown-content bg-base-200 rounded-box z-50 mt-2 w-40 p-2 shadow">
        {LOCALES.map((l) => (
          <li key={l}>
            <a
              href={localizeUrl(urlPathname, l)}
              hrefLang={LOCALE_INFO[l].htmlLang}
              lang={LOCALE_INFO[l].htmlLang}
              aria-current={l === locale ? "page" : undefined}
              className={l === locale ? "menu-active" : undefined}
            >
              {LOCALE_INFO[l].name}
            </a>
          </li>
        ))}
      </ul>
    </details>
  );
}

function Frame({ children }: { children: ReactNode }) {
  const { data, urlPathname, locale } = usePageContext();
  const { t } = useTranslation();
  const settings = (data as PageMeta | undefined)?.settings ?? FALLBACK;
  const active = (url: string) => url !== "/" && (urlPathname === url || urlPathname.startsWith(`${url}/`));
  return (
    // Visitors who ask for reduced motion get fades only.
    <MotionConfig reducedMotion="user">
      <div className="flex min-h-screen flex-col">
        <a href="#main" className="btn btn-sm absolute -top-20 left-2 z-50 focus:top-2">
          {t("layout.skipToContent")}
        </a>
        <header className="navbar bg-base-100/90 border-base-300 sticky top-0 z-40 border-b backdrop-blur">
          <div className="mx-auto flex w-full max-w-6xl items-center gap-2 px-2">
            <a href={localizeUrl("/", locale)} className="flex items-center gap-2 px-2 text-xl font-bold">
              <img src={logoUrl} width={32} height={32} alt="" />
              {settings.siteName}
            </a>
            <nav aria-label={t("layout.mainNavigation")} className="ml-auto hidden md:block">
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
            <LanguageMenu className="ml-auto md:ml-1" />
            <ButtonLink link={settings.cta} className="btn-primary btn-sm md:ml-1" />
            <details className="dropdown dropdown-end md:hidden">
              <summary className="btn btn-ghost btn-square" aria-label={t("layout.openNavigation")}>
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
          <PageFade key={urlPathname}>{children}</PageFade>
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
            <nav aria-label={t("layout.footerNavigation")}>
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
    </MotionConfig>
  );
}
