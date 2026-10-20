// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState } from "react";
import { Restricted } from "../../components/Restricted";
import { useQuery } from "../../lib/useQuery";
import { useViewer, type Abilities } from "../../lib/viewer";
import { discoverWorkflowTypes, type QueueEntry, type WorkflowStatus, type WorkflowType } from "../../lib/workflow";
import { EntryDrawer } from "./EntryDrawer";
import { QueueSection } from "./QueueSection";

export default function Page() {
  const { canSeeReviewQueue } = useViewer();
  return (
    <Restricted allowed={canSeeReviewQueue} what="see the review queue">
      <ReviewQueue />
    </Restricted>
  );
}

const TABS: { status: WorkflowStatus; label: string }[] = [
  { status: "IN_REVIEW", label: "In review" },
  { status: "CHANGES_REQUESTED", label: "Changes requested" },
];

/** Contributors and reviewers only see the content types in their scope (empty scope = all). */
function inScope(abilities: Abilities, type: WorkflowType): boolean {
  const { viewer } = abilities;
  if (viewer.kind !== "user" || abilities.isAdmin) return true;
  return viewer.user.scope.length === 0 || viewer.user.scope.includes(type.typeName);
}

function ReviewQueue() {
  const abilities = useViewer();
  const { data, error, loading, reload } = useQuery(discoverWorkflowTypes);
  const [status, setStatus] = useState<WorkflowStatus>(
    abilities.role === "CONTRIBUTOR" ? "CHANGES_REQUESTED" : "IN_REVIEW",
  );
  const [version, setVersion] = useState(0);
  const [open, setOpen] = useState<{ type: WorkflowType; entry: QueueEntry } | null>(null);
  const types = data?.filter((t) => inScope(abilities, t));

  return (
    <>
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-3xl font-bold">Review queue</h1>
        <button
          type="button"
          className="btn btn-ghost btn-sm"
          onClick={() => {
            reload();
            setVersion((v) => v + 1);
          }}
          disabled={loading}
        >
          {loading && <span className="loading loading-spinner loading-xs" aria-hidden="true" />}
          Refresh
        </button>
      </div>
      <p className="text-base-content/70 mb-6">
        Drafts submitted for review on content types with the review workflow. Approving publishes the draft; requesting
        changes sends it back to its author with your comment.
      </p>

      <div role="tablist" aria-label="Workflow status" className="tabs tabs-border mb-6">
        {TABS.map((tab) => (
          <button
            key={tab.status}
            type="button"
            role="tab"
            id={`tab-${tab.status}`}
            aria-selected={status === tab.status}
            aria-controls="review-panel"
            className={`tab ${status === tab.status ? "tab-active" : ""}`}
            onClick={() => setStatus(tab.status)}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div id="review-panel" role="tabpanel" aria-labelledby={`tab-${status}`}>
        {error && (
          <div role="alert" className="alert alert-error mb-4">
            <span>Could not discover the workflow content types: {error}</span>
            <button type="button" className="btn btn-sm" onClick={reload}>
              Retry
            </button>
          </div>
        )}
        {!data && loading && (
          <div className="flex justify-center py-12">
            <span className="loading loading-spinner loading-lg" aria-label="Loading the review queue" />
          </div>
        )}
        {types?.length === 0 && (
          <div className="border-base-300 rounded-box border border-dashed p-8 text-center">
            <h2 className="mb-2 text-lg font-semibold">No content type uses the review workflow</h2>
            <p className="text-base-content/70 text-sm">
              {abilities.isAdmin
                ? "Turn on the review workflow of a content type under Content models."
                : "Ask an administrator to enable the review workflow, or to add content types to your scope."}
            </p>
          </div>
        )}
        {types?.map((type) => (
          <QueueSection
            key={type.typeName}
            type={type}
            status={status}
            version={version}
            onOpen={(entry) => setOpen({ type, entry })}
          />
        ))}
      </div>

      {open && (
        <EntryDrawer
          key={open.entry.ref}
          type={open.type}
          entry={open.entry}
          canAct={status === "IN_REVIEW" && abilities.canReview(open.type.typeName)}
          onClose={() => setOpen(null)}
          onDone={() => setVersion((v) => v + 1)}
        />
      )}
    </>
  );
}
