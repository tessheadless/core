// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import logoUrl from "../assets/logo.svg";
import { usePageContext } from "vike-react/usePageContext";
import { LOCALE_INFO, LOCALES, localizeUrl } from "../lib/i18n";

const SHOW = ".motion-off [data-motion]";

/** Public origin of the website (`https://www.tessheadless.com`), for canonical and hreflang links. */
const SITE_URL = process.env.SITE_URL?.trim().replace(/\/$/, "");

/** Canonical URL and the same page in the other languages (search engines need absolute URLs). */
function Alternates() {
  const { urlPathname, locale, is404 } = usePageContext();
  if (!SITE_URL || is404) return null;
  return (
    <>
      <link rel="canonical" href={SITE_URL + localizeUrl(urlPathname, locale)} />
      {LOCALES.map((l) => (
        <link
          key={l}
          rel="alternate"
          hrefLang={LOCALE_INFO[l].htmlLang}
          href={SITE_URL + localizeUrl(urlPathname, l)}
        />
      ))}
      <link rel="alternate" hrefLang="x-default" href={SITE_URL + urlPathname} />
    </>
  );
}

// Rendered on the server only (prerendering).
export function Head() {
  return (
    <>
      <link rel="icon" href={logoUrl} />
      <Alternates />
      <meta name="theme-color" content="#0d3eb1" />
      {/* Animated elements are prerendered hidden: show them when JavaScript is off, or when the
          app has not started after 3 s (script blocked or failed to load). */}
      <style>{`${SHOW}{opacity:1!important;transform:none!important}`}</style>
      <noscript>
        <style>{"[data-motion]{opacity:1!important;transform:none!important}"}</style>
      </noscript>
      <script
        dangerouslySetInnerHTML={{
          __html:
            "setTimeout(function(){if(!window.__tessStarted)document.documentElement.classList.add('motion-off')},3000)",
        }}
      />
    </>
  );
}
