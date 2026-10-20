// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useId, type InputHTMLAttributes, type ReactNode } from "react";

interface TextFieldProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "id" | "className" | "onChange"> {
  label: ReactNode;
  value: string;
  onChange: (value: string) => void;
  /** Inline validation message (announced and linked to the input). */
  error?: string | null;
  hint?: ReactNode;
  mono?: boolean;
}

/** Labelled daisyUI input with accessible hint/error wiring. */
export function TextField({ label, value, onChange, error, hint, mono = false, ...rest }: TextFieldProps) {
  const id = useId();
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const describedBy = [hint ? hintId : null, error ? errorId : null].filter(Boolean).join(" ") || undefined;
  return (
    <fieldset className="fieldset">
      <label className="fieldset-label" htmlFor={id}>
        {label}
      </label>
      <input
        id={id}
        className={`input w-full ${mono ? "font-mono" : ""} ${error ? "input-error" : ""}`}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        aria-invalid={error ? true : undefined}
        aria-describedby={describedBy}
        {...rest}
      />
      {hint && (
        <p id={hintId} className="text-base-content/70 text-xs">
          {hint}
        </p>
      )}
      {error && (
        <p id={errorId} className="text-error text-sm">
          {error}
        </p>
      )}
    </fieldset>
  );
}

/** Spinner shown inside a busy button. */
export function ButtonSpinner({ busy }: { busy: boolean }) {
  return busy ? <span className="loading loading-spinner loading-sm" aria-hidden="true" /> : null;
}
