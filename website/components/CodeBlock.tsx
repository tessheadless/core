// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

export function CodeBlock({ code, language }: { code: string; language?: string | null }) {
  return (
    <div className="mockup-code bg-neutral text-neutral-content w-full overflow-x-auto text-sm shadow-xl">
      {language && <div className="text-neutral-content/60 px-6 pb-2 text-xs uppercase">{language}</div>}
      <pre className="px-6">
        <code>{code}</code>
      </pre>
    </div>
  );
}
