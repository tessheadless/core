// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useState, type FormEvent } from "react";
import { ConfirmModal } from "../../components/ConfirmModal";
import { ButtonSpinner, TextField } from "../../components/Field";
import { RoleBadge, ScopeList } from "../../components/RoleBadge";
import { Restricted } from "../../components/Restricted";
import { useToast } from "../../components/Toasts";
import { changePassword, passwordProblem, signOutEverywhere, type User } from "../../lib/accounts";
import { clearToken, setSession } from "../../lib/auth";
import { ApiError, describeError } from "../../lib/graphql";
import { formatDate } from "../../lib/format";
import { useViewer } from "../../lib/viewer";

export default function Page() {
  const { viewer } = useViewer();
  return (
    <Restricted allowed={viewer.kind === "user"} what="manage an account (you are signed in with an API token)">
      {viewer.kind === "user" && <Account user={viewer.user} />}
    </Restricted>
  );
}

function Account({ user }: { user: User }) {
  const toast = useToast();
  const [confirmSignOut, setConfirmSignOut] = useState(false);

  return (
    <>
      <h1 className="mb-6 text-3xl font-bold">My account</h1>
      <div className="grid gap-6 lg:grid-cols-2">
        <section className="card bg-base-100 border-base-300 border" aria-labelledby="profile-title">
          <div className="card-body">
            <h2 id="profile-title" className="card-title">
              Profile
            </h2>
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
              <dt className="text-base-content/70">Name</dt>
              <dd>{user.name}</dd>
              <dt className="text-base-content/70">Email</dt>
              <dd className="break-all">{user.email}</dd>
              <dt className="text-base-content/70">Role</dt>
              <dd>
                <RoleBadge role={user.role} />
              </dd>
              {(user.role === "CONTRIBUTOR" || user.role === "REVIEWER") && (
                <>
                  <dt className="text-base-content/70">Scope</dt>
                  <dd>
                    <ScopeList scope={user.scope} />
                  </dd>
                </>
              )}
              <dt className="text-base-content/70">Member since</dt>
              <dd>{formatDate(user.createdAt)}</dd>
            </dl>
            <p className="text-base-content/60 mt-2 text-xs">
              Ask an administrator to change your name, role or scope.
            </p>
          </div>
        </section>

        <ChangePasswordCard onChanged={() => toast.success("Password changed. Your other sessions were signed out.")} />

        <section className="card bg-base-100 border-base-300 border lg:col-span-2" aria-labelledby="sessions-title">
          <div className="card-body">
            <h2 id="sessions-title" className="card-title">
              Sessions
            </h2>
            <p className="text-base-content/70 text-sm">
              Signing out everywhere revokes every session of your account, including this one, on every device.
            </p>
            <div className="card-actions">
              <button type="button" className="btn btn-error btn-outline" onClick={() => setConfirmSignOut(true)}>
                Sign out everywhere
              </button>
            </div>
          </div>
        </section>
      </div>

      {confirmSignOut && (
        <ConfirmModal
          title="Sign out everywhere?"
          confirmLabel="Sign out everywhere"
          onClose={() => setConfirmSignOut(false)}
          onConfirm={async () => {
            await signOutEverywhere();
            clearToken("You were signed out of every device.");
          }}
        >
          All your sessions are revoked, including this one. You will need your password to sign in again.
        </ConfirmModal>
      )}
    </>
  );
}

interface PasswordErrors {
  current?: string;
  next?: string;
  confirm?: string;
  form?: string;
}

function ChangePasswordCard({ onChanged }: { onChanged: () => void }) {
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [errors, setErrors] = useState<PasswordErrors>({});
  const [busy, setBusy] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    const found: PasswordErrors = {};
    if (!current) found.current = "Enter your current password.";
    const weak = passwordProblem(next);
    if (weak) found.next = weak;
    else if (next === current) found.next = "Choose a password different from the current one.";
    else if (confirm !== next) found.confirm = "The passwords do not match.";
    setErrors(found);
    if (Object.keys(found).length > 0) return;

    setBusy(true);
    try {
      const session = await changePassword(current, next);
      // Every previous token was revoked: keep the new one.
      setSession(session.token, session.expiresAt);
      setCurrent("");
      setNext("");
      setConfirm("");
      onChanged();
    } catch (e) {
      if (e instanceof ApiError && (e.code === "FORBIDDEN" || e.code === "BAD_USER_INPUT"))
        setErrors({ form: e.message });
      else setErrors({ form: describeError(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="card bg-base-100 border-base-300 border" aria-labelledby="password-title">
      <form className="card-body" onSubmit={(e) => void submit(e)} noValidate>
        <h2 id="password-title" className="card-title">
          Change password
        </h2>
        <TextField
          label="Current password"
          type="password"
          autoComplete="current-password"
          value={current}
          onChange={setCurrent}
          error={errors.current}
          disabled={busy}
        />
        <TextField
          label="New password"
          type="password"
          autoComplete="new-password"
          value={next}
          onChange={setNext}
          error={errors.next}
          hint="At least 12 characters. Other sessions are signed out."
          disabled={busy}
        />
        <TextField
          label="Confirm new password"
          type="password"
          autoComplete="new-password"
          value={confirm}
          onChange={setConfirm}
          error={errors.confirm}
          disabled={busy}
        />
        {errors.form && (
          <div role="alert" className="alert alert-error text-sm">
            {errors.form}
          </div>
        )}
        <div className="card-actions mt-2">
          <button type="submit" className="btn btn-primary" disabled={busy}>
            <ButtonSpinner busy={busy} />
            Change password
          </button>
        </div>
      </form>
    </section>
  );
}
