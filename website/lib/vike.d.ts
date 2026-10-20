// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { Locale } from "./i18n";

declare global {
  namespace Vike {
    interface PageContext {
      /** Language of the page, from its URL (see pages/+onBeforeRoute.ts). */
      locale: Locale;
    }
  }
}

export {};
