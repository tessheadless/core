// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useId, useState, type FormEvent } from "react";
import { ButtonSpinner, TextField } from "../../components/Field";
import { Modal } from "../../components/Modal";
import { ScopeField } from "../../components/ScopeField";
import {
  createApiToken,
  ROLE_DESCRIPTION,
  ROLE_LABEL,
  roleUsesScope,
  type ApiToken,
  type Role,
} from "../../lib/accounts";
import { describeError } from "../../lib/graphql";

interface CreateTokenModalProps {
  grantableRoles: readonly Role[];
  contentTypeIds: string[];
  onClose: () => void;
  onCreated: (token: ApiToken) => void;
}

const MAX_DAYS = 3650;

export function CreateTokenModal({ grantableRoles, contentTypeIds, onClose, onCreated }: CreateTokenModalProps) {
  // The secret lives in this component's state only and disappears when the dialog closes.
  const [secret, setSecret] = useState<{ token: string; name: string } | null>(null);

  return (
    <Modal title={secret ? `Token "${secret.name}" created` : "Create an API token"} onClose={onClose} wide>
      {(close) =>
        secret ? (
          <ShowOnce token={secret.token} onDone={close} />
        ) : (
          <CreateForm
            grantableRoles={grantableRoles}
            contentTypeIds={contentTypeIds}
            onCancel={close}
            onCreated={(token, apiToken) => {
              onCreated(apiToken);
              setSecret({ token, name: apiToken.name });
            }}
          />
        )
      }
    </Modal>
  );
}

interface CreateFormProps {
  grantableRoles: readonly Role[];
  contentTypeIds: string[];
  onCancel: () => void;
  onCreated: (token: string, apiToken: ApiToken) => void;
}

function CreateForm({ grantableRoles, contentTypeIds, onCancel, onCreated }: CreateFormProps) {
  const roleId = useId();
  const roleHintId = useId();
  const [name, setName] = useState("");
  const [role, setRole] = useState<Role>(grantableRoles.includes("MEMBER") ? "MEMBER" : grantableRoles[0]);
  const [scope, setScope] = useState<string[]>([]);
  const [expires, setExpires] = useState<"never" | "days">("days");
  const [days, setDays] = useState("90");
  const [errors, setErrors] = useState<{ name?: string; days?: string; form?: string }>({});
  const [busy, setBusy] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    const found: typeof errors = {};
    if (!name.trim()) found.name = "Give the token a name, e.g. the site or script using it.";
    const dayCount = Number(days);
    if (expires === "days" && (!Number.isInteger(dayCount) || dayCount < 1 || dayCount > MAX_DAYS))
      found.days = `Enter a whole number of days between 1 and ${MAX_DAYS}.`;
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    setBusy(true);
    try {
      const created = await createApiToken({
        name: name.trim(),
        role,
        scope: roleUsesScope(role) ? scope : [],
        expiresInDays: expires === "days" ? dayCount : null,
      });
      onCreated(created.token, created.apiToken);
    } catch (e) {
      setErrors({ form: describeError(e) });
      setBusy(false);
    }
  }

  return (
    <form onSubmit={(e) => void submit(e)} noValidate>
      <TextField label="Name" autoComplete="off" value={name} onChange={setName} error={errors.name} disabled={busy} />
      <fieldset className="fieldset">
        <label className="fieldset-label" htmlFor={roleId}>
          Role
        </label>
        <select
          id={roleId}
          className="select w-full"
          value={role}
          onChange={(e) => setRole(e.target.value as Role)}
          disabled={busy}
          aria-describedby={roleHintId}
        >
          {grantableRoles.map((r) => (
            <option key={r} value={r}>
              {ROLE_LABEL[r]}
            </option>
          ))}
        </select>
        <p id={roleHintId} className="text-base-content/70 text-xs">
          {ROLE_DESCRIPTION[role]} Give tokens the smallest role that works.
        </p>
      </fieldset>
      {roleUsesScope(role) && <ScopeField options={contentTypeIds} value={scope} onChange={setScope} disabled={busy} />}
      <fieldset className="fieldset">
        <legend className="fieldset-legend">Expiry</legend>
        <div className="flex flex-wrap items-center gap-4">
          <label className="label cursor-pointer gap-2">
            <input
              type="radio"
              name="token-expiry"
              className="radio radio-sm"
              checked={expires === "days"}
              onChange={() => setExpires("days")}
              disabled={busy}
            />
            <span>Expires after</span>
          </label>
          <label className="input input-sm w-36">
            <input
              type="number"
              min={1}
              max={MAX_DAYS}
              value={days}
              onChange={(e) => {
                setDays(e.target.value);
                setExpires("days");
              }}
              disabled={busy}
              aria-label="Days before the token expires"
              aria-invalid={errors.days ? true : undefined}
            />
            <span className="label">days</span>
          </label>
          <label className="label cursor-pointer gap-2">
            <input
              type="radio"
              name="token-expiry"
              className="radio radio-sm"
              checked={expires === "never"}
              onChange={() => setExpires("never")}
              disabled={busy}
            />
            <span>Never expires</span>
          </label>
        </div>
        {errors.days && <p className="text-error text-sm">{errors.days}</p>}
      </fieldset>
      {errors.form && (
        <div role="alert" className="alert alert-error mt-3 text-sm">
          {errors.form}
        </div>
      )}
      <div className="modal-action">
        <button type="button" className="btn" onClick={onCancel}>
          Cancel
        </button>
        <button type="submit" className="btn btn-primary" disabled={busy}>
          <ButtonSpinner busy={busy} />
          Create token
        </button>
      </div>
    </form>
  );
}

function ShowOnce({ token, onDone }: { token: string; onDone: () => void }) {
  const inputId = useId();
  const [copied, setCopied] = useState<"yes" | "failed" | null>(null);

  async function copy() {
    try {
      await navigator.clipboard.writeText(token);
      setCopied("yes");
    } catch {
      setCopied("failed");
    }
  }

  return (
    <div>
      <div role="alert" className="alert alert-warning my-3 text-sm">
        <span>
          Copy this token now and store it in a secret manager. It is shown only once: after closing this dialog it
          cannot be displayed again (revoke it and create a new one if it is lost).
        </span>
      </div>
      <fieldset className="fieldset">
        <label className="fieldset-label" htmlFor={inputId}>
          API token
        </label>
        <div className="join w-full">
          <input
            id={inputId}
            type="text"
            readOnly
            className="input join-item w-full font-mono"
            value={token}
            spellCheck={false}
            autoComplete="off"
            onFocus={(e) => e.target.select()}
          />
          <button type="button" className="btn btn-primary join-item" onClick={() => void copy()}>
            Copy
          </button>
        </div>
        <p className="text-sm" aria-live="polite">
          {copied === "yes" && <span className="text-success">Copied to the clipboard.</span>}
          {copied === "failed" && (
            <span className="text-error">Copy failed: select the text and copy it manually.</span>
          )}
        </p>
      </fieldset>
      <p className="text-base-content/70 text-sm">
        Send it as <code className="font-mono">Authorization: Bearer &lt;token&gt;</code>.
      </p>
      <div className="modal-action">
        <button type="button" className="btn btn-primary" onClick={onDone}>
          I have stored the token
        </button>
      </div>
    </div>
  );
}
