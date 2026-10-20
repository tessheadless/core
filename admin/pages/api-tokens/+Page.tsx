// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState } from "react";
import { ActionButton } from "../../components/ActionButton";
import { ConfirmModal } from "../../components/ConfirmModal";
import { Restricted } from "../../components/Restricted";
import { RoleBadge, ScopeList } from "../../components/RoleBadge";
import { useToast } from "../../components/Toasts";
import { fetchTokensPage, revokeApiToken, ROLES, roleUsesScope, type ApiToken } from "../../lib/accounts";
import { formatDate } from "../../lib/format";
import { useQuery } from "../../lib/useQuery";
import { useViewer } from "../../lib/viewer";
import { CreateTokenModal } from "./CreateTokenModal";

export default function Page() {
  const { isAdmin } = useViewer();
  return (
    <Restricted allowed={isAdmin} what="manage API tokens">
      <Tokens />
    </Restricted>
  );
}

function Tokens() {
  const abilities = useViewer();
  const toast = useToast();
  const { data, error, loading, reload, setData } = useQuery(fetchTokensPage);
  const [creating, setCreating] = useState(false);
  const [revokingId, setRevokingId] = useState<string | null>(null);
  const grantableRoles = abilities.isOwner ? ROLES : ROLES.filter((r) => r !== "OWNER");
  const revoking = data?.apiTokens.find((t) => t.id === revokingId);
  // Reference time for the "Expired" badges, fixed when the page mounts.
  const [now] = useState(() => Date.now());

  return (
    <>
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-3xl font-bold">API tokens</h1>
        <div className="flex gap-2">
          <button type="button" className="btn btn-ghost btn-sm" onClick={reload} disabled={loading}>
            {loading && <span className="loading loading-spinner loading-xs" aria-hidden="true" />}
            Refresh
          </button>
          <button type="button" className="btn btn-primary btn-sm" onClick={() => setCreating(true)} disabled={!data}>
            Create token
          </button>
        </div>
      </div>
      <p className="text-base-content/70 mb-6">
        Tokens let scripts, front-ends and integrations call the API with a fixed role. The secret is shown only once,
        when the token is created.
      </p>

      {error && (
        <div role="alert" className="alert alert-error mb-4">
          <span>Could not load API tokens: {error}</span>
          <button type="button" className="btn btn-sm" onClick={reload}>
            Retry
          </button>
        </div>
      )}
      {!data && loading && (
        <div className="flex justify-center py-12">
          <span className="loading loading-spinner loading-lg" aria-label="Loading API tokens" />
        </div>
      )}

      {data && data.apiTokens.length === 0 && (
        <div className="border-base-300 rounded-box border border-dashed p-8 text-center">
          <h2 className="mb-2 text-lg font-semibold">No API tokens</h2>
          <p className="text-base-content/70 text-sm">Create one for each site or integration that calls the API.</p>
        </div>
      )}
      {data && data.apiTokens.length > 0 && (
        <div className="border-base-300 rounded-box overflow-x-auto border">
          <table className="table">
            <caption className="sr-only">API tokens</caption>
            <thead>
              <tr>
                <th scope="col">Name</th>
                <th scope="col">Role</th>
                <th scope="col">Scope</th>
                <th scope="col">Created</th>
                <th scope="col">Expires</th>
                <th scope="col">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {data.apiTokens.map((t) => (
                <TokenRow key={t.id} token={t} now={now} onRevoke={() => setRevokingId(t.id)} />
              ))}
            </tbody>
          </table>
        </div>
      )}

      {creating && data && (
        <CreateTokenModal
          grantableRoles={grantableRoles}
          contentTypeIds={data.contentTypes.map((t) => t.apiId)}
          onClose={() => setCreating(false)}
          onCreated={(token) => {
            setData((d) => ({ ...d, apiTokens: [...d.apiTokens, token] }));
            toast.success(`Token "${token.name}" created.`);
          }}
        />
      )}
      {revoking && (
        <ConfirmModal
          key={revoking.id}
          title={`Revoke "${revoking.name}"?`}
          confirmLabel="Revoke token"
          onClose={() => setRevokingId(null)}
          onConfirm={async () => {
            await revokeApiToken(revoking.id);
            setData((d) => ({ ...d, apiTokens: d.apiTokens.filter((t) => t.id !== revoking.id) }));
            toast.success(`Token "${revoking.name}" revoked.`);
          }}
        >
          Requests using this token are rejected immediately. This cannot be undone.
        </ConfirmModal>
      )}
    </>
  );
}

function TokenRow({ token, now, onRevoke }: { token: ApiToken; now: number; onRevoke: () => void }) {
  const expired = token.expiresAt !== null && Date.parse(token.expiresAt) <= now;
  return (
    <tr>
      <td className="font-semibold">{token.name}</td>
      <td>
        <RoleBadge role={token.role} />
      </td>
      <td>
        {roleUsesScope(token.role) ? <ScopeList scope={token.scope} /> : <span aria-label="Not applicable">—</span>}
      </td>
      <td className="whitespace-nowrap">
        {formatDate(token.createdAt)}
        <div className="text-base-content/60 text-xs">by {token.createdBy}</div>
      </td>
      <td className="whitespace-nowrap">
        {token.expiresAt ? formatDate(token.expiresAt) : "Never"}
        {expired && <span className="badge badge-error badge-soft badge-sm ml-2">Expired</span>}
      </td>
      {/* The button's text and sr-only suffix label it; the linter cannot see through ActionButton. */}
      {/* oxlint-disable-next-line jsx-a11y/control-has-associated-label */}
      <td>
        <div className="flex justify-end">
          <ActionButton reason={null} danger onClick={onRevoke}>
            Revoke<span className="sr-only"> {token.name}</span>
          </ActionButton>
        </div>
      </td>
    </tr>
  );
}
