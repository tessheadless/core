// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { usePageContext } from "vike-react/usePageContext";

export default function Page() {
  const { is404 } = usePageContext();
  return (
    <section className="mx-auto max-w-2xl px-4 py-24 text-center">
      <h1 className="text-4xl font-bold">{is404 ? "Page not found" : "Something went wrong"}</h1>
      <p className="text-base-content/70 mt-4">
        {is404 ? "This page does not exist, or it moved." : "Please try again in a moment."}
      </p>
      <a href="/" className="btn btn-primary mt-8">
        Back to the home page
      </a>
    </section>
  );
}
