// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { usePageContext } from "vike-react/usePageContext";

export default function Page() {
  const { is404, abortReason } = usePageContext();
  return (
    <main>
      <h1>{is404 ? "Not found" : "Something went wrong"}</h1>
      {typeof abortReason === "string" && <p>{abortReason}</p>}
      <a href="/">Back to the list</a>
    </main>
  );
}
