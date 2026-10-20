// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { PageContext } from "vike/types";
import { LOCALE_INFO } from "../lib/i18n";

export default function lang(pageContext: PageContext): string {
  return LOCALE_INFO[pageContext.locale].htmlLang;
}
