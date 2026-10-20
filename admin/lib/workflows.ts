// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { gql } from "./graphql";

export interface WorkflowEditor {
  provider: string;
  /** Same-origin URL of the editor, e.g. /n8n/. */
  url: string;
}

/** The license this server runs under; the commercial one has no workflow editor. */
export type License = "COMMUNITY" | "COMMERCIAL";

export function fetchWorkflowEditor() {
  return gql<{ workflowEditor: WorkflowEditor | null; license: License }>(
    `query WorkflowEditor { workflowEditor { provider url } license }`,
  );
}
