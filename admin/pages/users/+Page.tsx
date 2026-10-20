// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState } from "react";
import { ActionButton } from "../../components/ActionButton";
import { ConfirmModal } from "../../components/ConfirmModal";
import { Restricted } from "../../components/Restricted";
import { RoleBadge, ScopeList } from "../../components/RoleBadge";
import { useToast } from "../../components/Toasts";
import { deleteUser, fetchUsersPage, ROLES, roleUsesScope, type User } from "../../lib/accounts";
import { formatDate } from "../../lib/format";
import { useQuery } from "../../lib/useQuery";
import { useViewer, type Abilities } from "../../lib/viewer";
import { UserFormModal, type UserFormLocks } from "./UserFormModal";

export default function Page() {
  const { isAdmin } = useViewer();
  return (
    <Restricted allowed={isAdmin} what="manage users">
      <Users />
    </Restricted>
  );
}

interface Permissions {
  /** Why the row cannot be edited at all, or null. */
  edit: string | null;
  /** Why the user cannot be deleted, or null. */
  remove: string | null;
  locks: UserFormLocks;
}

/** Mirrors the server rules so the UI does not offer actions it would refuse. */
function permissionsFor(target: User, abilities: Abilities, activeOwners: number): Permissions {
  const self = target.id === abilities.userId;
  const lastOwner = target.role === "OWNER" && !target.disabled && activeOwners <= 1;
  if (target.role === "OWNER" && !abilities.isOwner) {
    const reason = "Only Owners can manage Owners.";
    return { edit: reason, remove: reason, locks: { role: reason, disable: reason } };
  }
  return {
    edit: null,
    remove: self ? "You cannot delete your own account." : lastOwner ? "There must always be an Owner." : null,
    locks: {
      role: self ? "You cannot change your own role." : lastOwner ? "There must always be an Owner." : null,
      disable: self ? "You cannot disable your own account." : lastOwner ? "There must always be an Owner." : null,
    },
  };
}

function Users() {
  const abilities = useViewer();
  const toast = useToast();
  const { data, error, loading, reload, setData } = useQuery(fetchUsersPage);
  const [creating, setCreating] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const grantableRoles = abilities.isOwner ? ROLES : ROLES.filter((r) => r !== "OWNER");
  const contentTypeIds = data?.contentTypes.map((t) => t.apiId) ?? [];
  const activeOwners = data?.users.filter((u) => u.role === "OWNER" && !u.disabled).length ?? 0;
  const editing = data?.users.find((u) => u.id === editingId);
  const deleting = data?.users.find((u) => u.id === deletingId);

  function upsert(user: User) {
    setData((d) => {
      const exists = d.users.some((u) => u.id === user.id);
      return { ...d, users: exists ? d.users.map((u) => (u.id === user.id ? user : u)) : [...d.users, user] };
    });
  }

  return (
    <>
      <div className="mb-6 flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-3xl font-bold">Users</h1>
        <div className="flex gap-2">
          <button type="button" className="btn btn-ghost btn-sm" onClick={reload} disabled={loading}>
            {loading && <span className="loading loading-spinner loading-xs" aria-hidden="true" />}
            Refresh
          </button>
          <button type="button" className="btn btn-primary btn-sm" onClick={() => setCreating(true)} disabled={!data}>
            Create user
          </button>
        </div>
      </div>

      {error && (
        <div role="alert" className="alert alert-error mb-4">
          <span>Could not load users: {error}</span>
          <button type="button" className="btn btn-sm" onClick={reload}>
            Retry
          </button>
        </div>
      )}
      {!data && loading && (
        <div className="flex justify-center py-12">
          <span className="loading loading-spinner loading-lg" aria-label="Loading users" />
        </div>
      )}

      {data && (
        <div className="border-base-300 rounded-box overflow-x-auto border">
          <table className="table">
            <caption className="sr-only">User accounts</caption>
            <thead>
              <tr>
                <th scope="col">Name</th>
                <th scope="col">Email</th>
                <th scope="col">Role</th>
                <th scope="col">Scope</th>
                <th scope="col">Status</th>
                <th scope="col">Last login</th>
                <th scope="col">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {data.users.map((u) => {
                const perms = permissionsFor(u, abilities, activeOwners);
                const self = u.id === abilities.userId;
                return (
                  <tr key={u.id}>
                    <td className="font-semibold">
                      {u.name}
                      {self && <span className="badge badge-ghost badge-xs ml-2">you</span>}
                    </td>
                    <td className="break-all">{u.email}</td>
                    <td>
                      <RoleBadge role={u.role} />
                    </td>
                    <td>
                      {roleUsesScope(u.role) ? (
                        <ScopeList scope={u.scope} />
                      ) : (
                        <span aria-label="Not applicable">—</span>
                      )}
                    </td>
                    <td>
                      {u.disabled ? (
                        <span className="badge badge-error badge-soft badge-sm">Disabled</span>
                      ) : (
                        <span className="badge badge-success badge-soft badge-sm">Active</span>
                      )}
                    </td>
                    <td className="whitespace-nowrap">{u.lastLoginAt ? formatDate(u.lastLoginAt) : "Never"}</td>
                    {/* Buttons are labelled by their text + sr-only suffix; the linter cannot see through ActionButton. */}
                    {/* oxlint-disable-next-line jsx-a11y/control-has-associated-label */}
                    <td>
                      <div className="flex justify-end gap-1">
                        <ActionButton reason={perms.edit} onClick={() => setEditingId(u.id)}>
                          Edit<span className="sr-only"> {u.name}</span>
                        </ActionButton>
                        <ActionButton reason={perms.remove} danger onClick={() => setDeletingId(u.id)}>
                          Delete<span className="sr-only"> {u.name}</span>
                        </ActionButton>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {creating && (
        <UserFormModal
          grantableRoles={grantableRoles}
          contentTypeIds={contentTypeIds}
          onClose={() => setCreating(false)}
          onSaved={upsert}
        />
      )}
      {editing && (
        <UserFormModal
          key={editing.id}
          user={editing}
          grantableRoles={grantableRoles}
          contentTypeIds={contentTypeIds}
          locks={permissionsFor(editing, abilities, activeOwners).locks}
          onClose={() => setEditingId(null)}
          onSaved={upsert}
        />
      )}
      {deleting && (
        <ConfirmModal
          key={deleting.id}
          title={`Delete ${deleting.name}?`}
          confirmLabel="Delete user"
          onClose={() => setDeletingId(null)}
          onConfirm={async () => {
            await deleteUser(deleting.id);
            setData((d) => ({ ...d, users: d.users.filter((u) => u.id !== deleting.id) }));
            toast.success(`${deleting.name} deleted.`);
          }}
        >
          The account <strong className="break-all">{deleting.email}</strong> is removed and its sessions stop working.
          Content they wrote is kept. To block access temporarily, disable the account instead.
        </ConfirmModal>
      )}
    </>
  );
}
