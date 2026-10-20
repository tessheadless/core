// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useId } from "react";
import { I18N_SCOPE } from "../lib/viewer";

interface ScopeFieldProps {
  /** Every content type apiId known to the server. */
  options: string[];
  value: string[];
  onChange: (scope: string[]) => void;
  disabled?: boolean;
}

/** Multi-select of content types as a checkbox group. Nothing checked = all content types. */
export function ScopeField({ options, value, onChange, disabled = false }: ScopeFieldProps) {
  const hintId = useId();
  // Translations are granted like a content type, with the `i18n` pseudo type.
  const all = [...new Set([...options, ...value, I18N_SCOPE])].sort((a, b) => a.localeCompare(b));

  function toggle(apiId: string, checked: boolean) {
    onChange(checked ? [...value, apiId] : value.filter((v) => v !== apiId));
  }

  return (
    <fieldset className="fieldset" aria-describedby={hintId}>
      <legend className="fieldset-legend">Scope</legend>
      <p id={hintId} className="text-base-content/70 text-xs">
        Content types this account may work on (`i18n` = translations). Leave everything unchecked to allow everything.
      </p>
      {all.length === 0 ? (
        <p className="text-base-content/60 text-sm">No content types defined yet.</p>
      ) : (
        <div className="grid max-h-48 grid-cols-1 gap-1 overflow-y-auto sm:grid-cols-2">
          {all.map((apiId) => (
            <label key={apiId} className="label cursor-pointer justify-start gap-2 py-1">
              <input
                type="checkbox"
                className="checkbox checkbox-sm"
                checked={value.includes(apiId)}
                onChange={(e) => toggle(apiId, e.target.checked)}
                disabled={disabled}
              />
              <span className="font-mono text-sm">{apiId}</span>
              {apiId === I18N_SCOPE && <span className="text-base-content/60 text-xs">(translations)</span>}
            </label>
          ))}
        </div>
      )}
    </fieldset>
  );
}
