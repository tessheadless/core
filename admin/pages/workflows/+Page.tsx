// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { withBase } from "../../components/Link";
import { Restricted } from "../../components/Restricted";
import { useQuery } from "../../lib/useQuery";
import { useViewer } from "../../lib/viewer";
import { fetchWorkflowEditor } from "../../lib/workflows";

export default function Page() {
  const { isAdmin, role } = useViewer();
  return (
    <Restricted allowed={isAdmin || role === "CONTRIBUTOR"} what="build workflows">
      <Workflows />
    </Restricted>
  );
}

function Workflows() {
  const { isAdmin } = useViewer();
  const { data, error, loading } = useQuery(fetchWorkflowEditor);
  const editor = data?.workflowEditor;

  if (loading && !data) {
    return (
      <div className="flex justify-center py-12">
        <span className="loading loading-spinner loading-lg" aria-label="Loading" />
      </div>
    );
  }
  if (error) {
    return (
      <div role="alert" className="alert alert-error">
        <span>{error}</span>
      </div>
    );
  }
  if (data?.license === "COMMERCIAL") return <NotIncluded />;
  if (!editor) return <Setup isAdmin={isAdmin} />;

  return (
    <div className="-m-4 flex h-[calc(100vh-4rem)] flex-col lg:-m-8 lg:h-screen">
      <div className="bg-base-200 border-base-300 flex flex-wrap items-center gap-3 border-b px-4 py-2">
        <h1 className="text-lg font-semibold">Workflows</h1>
        <span className="text-base-content/60 hidden text-xs md:inline">
          Start a workflow with the <strong>Tess Trigger</strong> node; read and write content with the{" "}
          <strong>Tess</strong> node. n8n has its own accounts.
        </span>
        <div className="ml-auto flex gap-2">
          {isAdmin && (
            <a className="btn btn-xs btn-ghost" href={withBase("/webhooks")}>
              Webhooks
            </a>
          )}
          <a className="btn btn-xs" href={editor.url} target="_blank" rel="noreferrer">
            Open in a new tab ↗
          </a>
        </div>
      </div>
      <iframe
        title="n8n workflow editor"
        src={editor.url}
        className="w-full flex-1 border-0"
        allow="clipboard-read; clipboard-write"
      />
    </div>
  );
}

function NotIncluded() {
  return (
    <div className="prose max-w-3xl">
      <h1>Workflows</h1>
      <p>
        The embedded n8n workflow editor isn&apos;t available with the commercial license of Tess. Webhooks still send
        content, model and translation events to the automation tools of your choice.
      </p>
    </div>
  );
}

function Setup({ isAdmin }: { isAdmin: boolean }) {
  return (
    <div className="prose max-w-3xl">
      <h1>Workflows</h1>
      <p>
        Automate Tess with <a href="https://n8n.io">n8n</a>: react to content, model and translation events, call the
        Tess GraphQL API, and connect hundreds of other services.
      </p>
      {isAdmin ? (
        <>
          <p>No workflow engine is configured on this server yet. With Docker Compose:</p>
          <pre>
            <code>docker compose --profile n8n up -d</code>
          </pre>
          <p>
            and set <code>TESS_N8N_URL=http://n8n:5678</code> for the Tess service (n8n runs under the{" "}
            <code>/n8n/</code> path, proxied by Tess). Restart Tess and this page shows the editor.
          </p>
          <p>Then, in n8n:</p>
          <ol>
            <li>
              Create a <strong>Tess API</strong> credential: the Tess URL as n8n sees it (e.g.{" "}
              <code>http://tess:8080</code>) and an <a href={withBase("/api-tokens")}>API token</a> (Admin role, so the
              trigger can register its webhook).
            </li>
            <li>
              Add a <strong>Tess Trigger</strong> node and pick events: it registers a{" "}
              <a href={withBase("/webhooks")}>webhook</a> when the workflow is activated and removes it when
              deactivated.
            </li>
          </ol>
        </>
      ) : (
        <p>Ask an Admin to set up the workflow engine.</p>
      )}
    </div>
  );
}
