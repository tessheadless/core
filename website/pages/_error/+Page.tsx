// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useTranslation } from "react-i18next";
import { usePageContext } from "vike-react/usePageContext";
import { A } from "../../components/SiteLink";

export default function Page() {
  const { is404 } = usePageContext();
  const { t } = useTranslation();
  return (
    <section className="mx-auto max-w-2xl px-4 py-24 text-center">
      <h1 className="text-4xl font-bold">{t(is404 ? "error.notFoundTitle" : "error.errorTitle")}</h1>
      <p className="text-base-content/70 mt-4">{t(is404 ? "error.notFoundBody" : "error.errorBody")}</p>
      <A href="/" className="btn btn-primary mt-8">
        {t("error.backHome")}
      </A>
    </section>
  );
}
