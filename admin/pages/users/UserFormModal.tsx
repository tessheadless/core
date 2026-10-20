// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useId, useState, type FormEvent } from "react";
import { ButtonSpinner, TextField } from "../../components/Field";
import { Modal } from "../../components/Modal";
import { ScopeField } from "../../components/ScopeField";
import { useToast } from "../../components/Toasts";
import {
  createUser,
  EMAIL_PATTERN,
  passwordProblem,
  ROLE_DESCRIPTION,
  ROLE_LABEL,
  roleUsesScope,
  updateUser,
  type Role,
  type User,
  type UserUpdateInput,
} from "../../lib/accounts";
import { describeError } from "../../lib/graphql";

export interface UserFormLocks {
  /** Why the role cannot be changed (self, last Owner…), or null. */
  role: string | null;
  /** Why the account cannot be disabled, or null. */
  disable: string | null;
}

interface UserFormModalProps {
  /** Edit this user; omitted = create. */
  user?: User;
  grantableRoles: readonly Role[];
  contentTypeIds: string[];
  locks?: UserFormLocks;
  onClose: () => void;
  onSaved: (user: User) => void;
}

interface Errors {
  name?: string;
  email?: string;
  password?: string;
  form?: string;
}

function sameScope(a: string[], b: string[]) {
  return a.length === b.length && [...a].sort().join("\n") === [...b].sort().join("\n");
}

export function UserFormModal({ user, grantableRoles, contentTypeIds, locks, onClose, onSaved }: UserFormModalProps) {
  const toast = useToast();
  const roleId = useId();
  const roleHintId = useId();
  const editing = user !== undefined;
  const [name, setName] = useState(user?.name ?? "");
  const [email, setEmail] = useState(user?.email ?? "");
  const [role, setRole] = useState<Role>(user?.role ?? "CONTRIBUTOR");
  const [scope, setScope] = useState<string[]>(user?.scope ?? []);
  const [disabled, setDisabled] = useState(user?.disabled ?? false);
  const [password, setPassword] = useState("");
  const [errors, setErrors] = useState<Errors>({});
  const [busy, setBusy] = useState(false);

  // The current role stays selectable even if the viewer could not grant it.
  const roleOptions = user && !grantableRoles.includes(user.role) ? [user.role, ...grantableRoles] : grantableRoles;
  const effectiveScope = roleUsesScope(role) ? scope : [];

  function validate(): Errors {
    const found: Errors = {};
    if (!name.trim()) found.name = "Enter a name.";
    if (!editing && !EMAIL_PATTERN.test(email.trim())) found.email = "Enter a valid email address.";
    if (!editing || password) {
      const weak = passwordProblem(password);
      if (weak) found.password = weak;
    }
    return found;
  }

  function changes(original: User): UserUpdateInput {
    const input: UserUpdateInput = {};
    if (name.trim() !== original.name) input.name = name.trim();
    if (role !== original.role) input.role = role;
    if (!sameScope(effectiveScope, original.scope)) input.scope = effectiveScope;
    if (disabled !== original.disabled) input.disabled = disabled;
    if (password) input.password = password;
    return input;
  }

  async function submit(event: FormEvent, close: () => void) {
    event.preventDefault();
    const found = validate();
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    setBusy(true);
    try {
      let saved: User;
      if (user) {
        const input = changes(user);
        if (Object.keys(input).length === 0) {
          close();
          return;
        }
        saved = await updateUser(user.id, input);
        toast.success(`${saved.name} updated.`);
      } else {
        saved = await createUser({ email: email.trim(), name: name.trim(), role, scope: effectiveScope, password });
        toast.success(`${saved.name} can now sign in.`);
      }
      onSaved(saved);
      close();
    } catch (e) {
      // Shown inline: toasts sit below the modal's top layer.
      setErrors({ form: describeError(e) });
      setBusy(false);
    }
  }

  return (
    <Modal title={editing ? `Edit ${user.name}` : "Create a user"} onClose={onClose} wide>
      {(close) => (
        <form onSubmit={(e) => void submit(e, close)} noValidate>
          <div className="grid gap-x-4 sm:grid-cols-2">
            <TextField
              label="Name"
              autoComplete="off"
              value={name}
              onChange={setName}
              error={errors.name}
              disabled={busy}
            />
            <TextField
              label="Email"
              type="email"
              autoComplete="off"
              value={email}
              onChange={setEmail}
              error={errors.email}
              disabled={busy || editing}
              hint={editing ? "The email cannot be changed." : undefined}
            />
          </div>

          <fieldset className="fieldset">
            <label className="fieldset-label" htmlFor={roleId}>
              Role
            </label>
            <select
              id={roleId}
              className="select w-full"
              value={role}
              onChange={(e) => setRole(e.target.value as Role)}
              disabled={busy || locks?.role != null}
              aria-describedby={roleHintId}
            >
              {roleOptions.map((r) => (
                <option key={r} value={r} disabled={!grantableRoles.includes(r) && r !== user?.role}>
                  {ROLE_LABEL[r]}
                </option>
              ))}
            </select>
            <p id={roleHintId} className="text-base-content/70 text-xs">
              {locks?.role ?? ROLE_DESCRIPTION[role]}
            </p>
          </fieldset>

          {roleUsesScope(role) && (
            <ScopeField options={contentTypeIds} value={scope} onChange={setScope} disabled={busy} />
          )}

          {editing && (
            <fieldset className="fieldset">
              <label className="label cursor-pointer justify-start gap-3">
                <input
                  type="checkbox"
                  className="toggle toggle-error toggle-sm"
                  checked={disabled}
                  onChange={(e) => setDisabled(e.target.checked)}
                  disabled={busy || locks?.disable != null}
                />
                <span>Disabled (cannot sign in; existing sessions stop working)</span>
              </label>
              {locks?.disable && <p className="text-base-content/70 text-xs">{locks.disable}</p>}
            </fieldset>
          )}

          <TextField
            label={editing ? "Reset password (optional)" : "Initial password"}
            type="password"
            autoComplete="new-password"
            value={password}
            onChange={setPassword}
            error={errors.password}
            hint={
              editing
                ? "Leave empty to keep the current password. At least 12 characters."
                : "At least 12 characters. Share it securely; the user can change it under My account."
            }
            disabled={busy}
          />

          {errors.form && (
            <div role="alert" className="alert alert-error mt-3 text-sm">
              {errors.form}
            </div>
          )}
          <div className="modal-action">
            <button type="button" className="btn" onClick={close}>
              Cancel
            </button>
            <button type="submit" className="btn btn-primary" disabled={busy}>
              <ButtonSpinner busy={busy} />
              {editing ? "Save" : "Create user"}
            </button>
          </div>
        </form>
      )}
    </Modal>
  );
}
