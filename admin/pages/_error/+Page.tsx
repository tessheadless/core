// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { usePageContext } from "vike-react/usePageContext";
import { withBase } from "../../components/Link";

export default function Page() {
  const { is404 } = usePageContext();
  return (
    <div className="py-12 text-center">
      <h1 className="mb-2 text-3xl font-bold">{is404 ? "Page not found" : "Something went wrong"}</h1>
      <p className="text-base-content/70 mb-6">
        {is404 ? "This page could not be found." : "An unexpected error occurred."}
      </p>
      <a href={withBase("/")} className="btn btn-primary">
        Back to the dashboard
      </a>
    </div>
  );
}
