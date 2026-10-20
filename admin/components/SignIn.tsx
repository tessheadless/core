// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import logoUrl from "../assets/logo.svg";
import { createUser, EMAIL_PATTERN, fetchSetupRequired, login, passwordProblem } from "../lib/accounts";
import { getSignOutReason, setFlash, setSession, setToken } from "../lib/auth";
import { ApiError, describeError, gql } from "../lib/graphql";
import { ButtonSpinner, TextField } from "./Field";

type SetupState = "checking" | "required" | "not-required" | "unknown";

/** Shown when signed out: first-run setup, email/password sign-in, or a pasted API token. */
export function SignIn() {
  const [setup, setSetup] = useState<SetupState>("checking");
  const [mode, setMode] = useState<"password" | "token">("password");
  const [notice, setNotice] = useState<string | null>(getSignOutReason());

  useEffect(() => {
    let cancelled = false;
    fetchSetupRequired().then(
      (required) => {
        if (!cancelled) setSetup(required ? "required" : "not-required");
      },
      () => {
        if (!cancelled) setSetup("unknown");
      },
    );
    return () => {
      cancelled = true;
    };
  }, []);

  if (setup === "checking") {
    return (
      <AuthCard title="Tess Admin">
        <div className="flex justify-center py-6">
          <span className="loading loading-spinner loading-lg" aria-label="Loading" />
        </div>
      </AuthCard>
    );
  }

  if (setup === "required") {
    return (
      <SetupOwner
        onAccountCreated={(message) => {
          // The Owner exists but the automatic sign-in failed: fall back to the normal form.
          setNotice(message);
          setSetup("not-required");
        }}
      />
    );
  }

  return (
    <AuthCard title="Sign in">
      {notice && <output className="alert alert-warning alert-soft text-sm">{notice}</output>}
      {setup === "unknown" && (
        <output className="alert alert-info alert-soft text-sm">
          Could not reach the server to check its setup state. You can still try to sign in.
        </output>
      )}
      {mode === "password" ? <PasswordForm /> : <TokenForm />}
      <button
        type="button"
        className="btn btn-link btn-sm self-center"
        onClick={() => setMode((m) => (m === "password" ? "token" : "password"))}
      >
        {mode === "password" ? "Use an API/bootstrap token instead" : "Sign in with email and password"}
      </button>
    </AuthCard>
  );
}

function AuthCard({ title, children }: { title: string; children: ReactNode }) {
  return (
    <main className="bg-base-200 flex min-h-screen items-center justify-center p-4">
      <div className="card bg-base-100 w-full max-w-md shadow-xl">
        <div className="card-body gap-4">
          <div className="flex items-center gap-3">
            <img src={logoUrl} width={40} height={40} alt="" />
            <h1 className="card-title text-2xl">{title}</h1>
          </div>
          {children}
        </div>
      </div>
    </main>
  );
}

function loginErrorMessage(error: unknown): string {
  if (
    error instanceof ApiError &&
    (error.status === 401 || error.code === "FORBIDDEN" || error.code === "UNAUTHENTICATED")
  )
    return "Incorrect email or password, or the account is disabled.";
  return describeError(error);
}

function PasswordForm() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!email.trim() || !password) {
      setError("Enter your email and password.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const session = await login(email.trim(), password);
      setSession(session.token, session.expiresAt);
    } catch (e) {
      setError(loginErrorMessage(e));
      setBusy(false);
    }
  }

  return (
    <form className="flex flex-col gap-2" onSubmit={(e) => void submit(e)} noValidate>
      <p className="text-base-content/70 text-sm">
        Your session is kept for this browser tab only and forgotten when the tab closes.
      </p>
      <TextField
        label="Email"
        type="email"
        autoComplete="username"
        value={email}
        onChange={setEmail}
        disabled={busy}
        required
      />
      <TextField
        label="Password"
        type="password"
        autoComplete="current-password"
        value={password}
        onChange={setPassword}
        disabled={busy}
        required
      />
      {error && (
        <p role="alert" className="text-error text-sm">
          {error}
        </p>
      )}
      <button type="submit" className="btn btn-primary mt-2" disabled={busy}>
        <ButtonSpinner busy={busy} />
        {busy ? "Signing in…" : "Sign in"}
      </button>
    </form>
  );
}

function TokenForm() {
  const [token, setTokenInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    const candidate = token.trim();
    if (!candidate) {
      setError("Enter an API token.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      // Any valid credential can run `me` (null for tokens); invalid ones get HTTP 401.
      await gql<{ me: { id: string } | null }>("query TokenCheck { me { id } }", undefined, { token: candidate });
      setToken(candidate);
    } catch (e) {
      setError(e instanceof ApiError && e.status === 401 ? "This token was rejected by the server." : describeError(e));
      setBusy(false);
    }
  }

  return (
    <form className="flex flex-col gap-2" onSubmit={(e) => void submit(e)} noValidate>
      <p className="text-base-content/70 text-sm">
        Paste an API token or the bootstrap admin token configured on the server. It is kept for this browser tab only.
      </p>
      <TextField
        label="API token"
        type="password"
        autoComplete="off"
        spellCheck={false}
        value={token}
        onChange={setTokenInput}
        error={error}
        disabled={busy}
        mono
      />
      <button type="submit" className="btn btn-primary mt-2" disabled={busy}>
        <ButtonSpinner busy={busy} />
        {busy ? "Checking…" : "Sign in with token"}
      </button>
    </form>
  );
}

interface SetupErrors {
  token?: string;
  name?: string;
  email?: string;
  password?: string;
  confirm?: string;
  form?: string;
}

const REMOVE_BOOTSTRAP_TOKEN =
  "Your Owner account is ready. Remove TESS_ADMIN_TOKEN from the server configuration and restart the server: the bootstrap token grants full access and is no longer needed.";

function SetupOwner({ onAccountCreated }: { onAccountCreated: (message: string) => void }) {
  const [bootstrapToken, setBootstrapToken] = useState("");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [errors, setErrors] = useState<SetupErrors>({});
  const [busy, setBusy] = useState(false);

  function validate(): SetupErrors {
    const found: SetupErrors = {};
    if (!bootstrapToken.trim()) found.token = "Enter the bootstrap admin token (TESS_ADMIN_TOKEN).";
    if (!name.trim()) found.name = "Enter your name.";
    if (!EMAIL_PATTERN.test(email.trim())) found.email = "Enter a valid email address.";
    const weak = passwordProblem(password);
    if (weak) found.password = weak;
    else if (confirm !== password) found.confirm = "The passwords do not match.";
    return found;
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    const found = validate();
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    setBusy(true);
    try {
      await createUser(
        { email: email.trim(), name: name.trim(), role: "OWNER", scope: [], password },
        bootstrapToken.trim(),
      );
    } catch (e) {
      setErrors(
        e instanceof ApiError && e.status === 401
          ? { token: "The bootstrap token was rejected by the server." }
          : { form: describeError(e) },
      );
      setBusy(false);
      return;
    }
    try {
      const session = await login(email.trim(), password);
      setFlash(REMOVE_BOOTSTRAP_TOKEN);
      setSession(session.token, session.expiresAt);
    } catch (e) {
      onAccountCreated(`Owner account created, but signing in failed (${describeError(e)}). ${REMOVE_BOOTSTRAP_TOKEN}`);
    }
  }

  return (
    <AuthCard title="Create the first Owner">
      <p className="text-base-content/70 text-sm">
        This CMS has no user accounts yet. Use the bootstrap admin token from the server configuration (
        <code className="font-mono">TESS_ADMIN_TOKEN</code>) to create the first Owner account.
      </p>
      <form className="flex flex-col gap-1" onSubmit={(e) => void submit(e)} noValidate>
        <TextField
          label="Bootstrap admin token"
          type="password"
          autoComplete="off"
          spellCheck={false}
          value={bootstrapToken}
          onChange={setBootstrapToken}
          error={errors.token}
          disabled={busy}
          mono
        />
        <TextField
          label="Your name"
          autoComplete="name"
          value={name}
          onChange={setName}
          error={errors.name}
          disabled={busy}
        />
        <TextField
          label="Email"
          type="email"
          autoComplete="username"
          value={email}
          onChange={setEmail}
          error={errors.email}
          disabled={busy}
        />
        <TextField
          label="Password"
          type="password"
          autoComplete="new-password"
          value={password}
          onChange={setPassword}
          error={errors.password}
          hint="At least 12 characters."
          disabled={busy}
        />
        <TextField
          label="Confirm password"
          type="password"
          autoComplete="new-password"
          value={confirm}
          onChange={setConfirm}
          error={errors.confirm}
          disabled={busy}
        />
        {errors.form && (
          <div role="alert" className="alert alert-error mt-2 text-sm">
            {errors.form}
          </div>
        )}
        <button type="submit" className="btn btn-primary mt-3" disabled={busy}>
          <ButtonSpinner busy={busy} />
          {busy ? "Creating…" : "Create Owner and sign in"}
        </button>
      </form>
    </AuthCard>
  );
}
