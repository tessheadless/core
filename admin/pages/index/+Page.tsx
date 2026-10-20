// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { withBase } from "../../components/Link";
import { ROLE_DESCRIPTION } from "../../lib/accounts";
import { useViewer, type Abilities } from "../../lib/viewer";

interface Section {
  href: string;
  title: string;
  text: string;
  visible: (a: Abilities) => boolean;
}

const SECTIONS: Section[] = [
  {
    href: "/review",
    title: "Review queue",
    text: "Entries waiting for review, and the ones sent back with requested changes.",
    visible: (a) => a.canSeeReviewQueue,
  },
  {
    href: "/content-models",
    title: "Content models",
    text: "The content types exposed by the GraphQL API, their fields and review workflow.",
    visible: (a) => a.isAdmin,
  },
  {
    href: "/users",
    title: "Users",
    text: "Accounts, roles and the content types each contributor or reviewer may work on.",
    visible: (a) => a.isAdmin,
  },
  {
    href: "/api-tokens",
    title: "API tokens",
    text: "Credentials for scripts, front-ends and integrations.",
    visible: (a) => a.isAdmin,
  },
  {
    href: "/plugins",
    title: "Plugins",
    text: "WebAssembly hooks that validate or react to content writes.",
    visible: (a) => a.isAdmin,
  },
  {
    href: "/account",
    title: "My account",
    text: "Change your password or sign out of every device.",
    visible: (a) => a.viewer.kind === "user",
  },
];

export default function Page() {
  const abilities = useViewer();
  const sections = SECTIONS.filter((s) => s.visible(abilities));
  const { viewer } = abilities;

  return (
    <>
      <h1 className="mb-2 text-3xl font-bold">Dashboard</h1>
      <p className="text-base-content/70 mb-6">
        {viewer.kind === "user"
          ? `Signed in as ${viewer.user.name}. ${ROLE_DESCRIPTION[viewer.user.role]}`
          : "Signed in with an API token."}
      </p>
      <div className="grid gap-4 sm:grid-cols-2">
        {sections.map((s) => (
          <div key={s.href} className="card bg-base-100 border-base-300 relative border hover:shadow-md">
            <div className="card-body">
              <h2 className="card-title">
                <a href={withBase(s.href)} className="link link-hover after:absolute after:inset-0">
                  {s.title}
                </a>
              </h2>
              <p className="text-base-content/70">{s.text}</p>
            </div>
          </div>
        ))}
      </div>
    </>
  );
}
