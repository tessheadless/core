// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import logoUrl from "../assets/logo.svg";

export function Head() {
  return (
    <>
      <link rel="icon" href={logoUrl} />
      <meta name="theme-color" content="#0d3eb1" />
    </>
  );
}
