// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useData } from "vike-react/useData";
import { Sections } from "../../components/Sections";
import type { Data } from "./+data";

export default function Page() {
  const { page, context } = useData<Data>();
  return <Sections sections={page.sections} context={context} />;
}
