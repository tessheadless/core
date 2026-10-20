// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import logoUrl from "../assets/logo.svg";

const SHOW = ".motion-off [data-motion]";

export function Head() {
  return (
    <>
      <link rel="icon" href={logoUrl} />
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
