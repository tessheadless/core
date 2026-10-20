// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { createContext, useContext, type ReactNode } from "react";
import type { Role, User } from "./accounts";

/**
 * Who is signed in. User sessions know their role; API / bootstrap tokens have no
 * `me`, so we only know whether they can use admin queries (probed at sign-in).
 */
export type Viewer = { kind: "user"; user: User } | { kind: "token"; admin: boolean };

export interface Abilities {
  viewer: Viewer;
  /** null for tokens (unknown role). */
  role: Role | null;
  userId: string | null;
  /** Owner/Admin user, or a token that may query admin data. */
  isAdmin: boolean;
  /** Owner user, or an admin token (the bootstrap token acts as an Owner; the server has the last word). */
  isOwner: boolean;
  /** May open the review queue (Owner/Admin/Reviewer; contributors see it read-only). */
  canSeeReviewQueue: boolean;
  /** May approve / request changes on this content type (best effort: the server enforces it). */
  canReview: (apiId: string) => boolean;
  /** Content manager, mirroring the server rules (crates/core/src/access.rs). */
  content: ContentAbilities;
  /** Translations, mirroring crates/i18n/src/service.rs (scope pseudo type `i18n`). */
  translations: TranslationAbilities;
}

/** Scope entry that grants access to translations (like a content type apiId). */
export const I18N_SCOPE = "i18n";

export interface TranslationAbilities {
  /** Create keys, edit texts, import (Owner/Admin; Contributor within scope). */
  canEdit: boolean;
  /** Mark texts as reviewed (Owner/Admin; Reviewer within scope). */
  canReview: boolean;
}

/** What the viewer may do with entries of a content type. */
export interface ContentAbilities {
  /** Create and edit drafts (Owner/Admin; Contributor within scope). */
  canEdit: (apiId: string) => boolean;
  /** Publish / unpublish directly (Owner/Admin; Contributor on types without workflow). */
  canPublish: (apiId: string, workflow: boolean) => boolean;
  /** Submit for review (Contributor on workflow types; Admins may too). */
  canSubmit: (apiId: string, workflow: boolean) => boolean;
  /** Delete (Owner/Admin; Contributor only entries never published). */
  canDelete: (apiId: string, everPublished: boolean) => boolean;
}

const NO_CONTENT: ContentAbilities = {
  canEdit: () => false,
  canPublish: () => false,
  canSubmit: () => false,
  canDelete: () => false,
};

const ALL_CONTENT: ContentAbilities = {
  canEdit: () => true,
  canPublish: () => true,
  canSubmit: (_, workflow) => workflow,
  canDelete: () => true,
};

export function abilitiesFor(viewer: Viewer): Abilities {
  if (viewer.kind === "token") {
    return {
      viewer,
      role: null,
      userId: null,
      isAdmin: viewer.admin,
      isOwner: viewer.admin,
      canSeeReviewQueue: true,
      canReview: () => true,
      // A token's exact role is unknown here: only admin tokens edit from the UI.
      content: viewer.admin ? ALL_CONTENT : NO_CONTENT,
      translations: { canEdit: viewer.admin, canReview: viewer.admin },
    };
  }
  const { role, scope, id } = viewer.user;
  const isAdmin = role === "OWNER" || role === "ADMIN";
  const inScope = (apiId: string) => scope.length === 0 || scope.includes(apiId);
  const contributor = (apiId: string) => role === "CONTRIBUTOR" && inScope(apiId);
  const content: ContentAbilities = isAdmin
    ? ALL_CONTENT
    : role === "CONTRIBUTOR"
      ? {
          canEdit: contributor,
          canPublish: (apiId, workflow) => contributor(apiId) && !workflow,
          canSubmit: (apiId, workflow) => contributor(apiId) && workflow,
          canDelete: (apiId, everPublished) => contributor(apiId) && !everPublished,
        }
      : NO_CONTENT; // Reviewers and Members read only.
  return {
    viewer,
    role,
    userId: id,
    isAdmin,
    isOwner: role === "OWNER",
    canSeeReviewQueue: isAdmin || role === "REVIEWER" || role === "CONTRIBUTOR",
    canReview: (apiId) => isAdmin || (role === "REVIEWER" && (scope.length === 0 || scope.includes(apiId))),
    content,
    translations: {
      canEdit: isAdmin || (role === "CONTRIBUTOR" && inScope(I18N_SCOPE)),
      canReview: isAdmin || (role === "REVIEWER" && inScope(I18N_SCOPE)),
    },
  };
}

const ViewerContext = createContext<Abilities | null>(null);

export function ViewerProvider({ abilities, children }: { abilities: Abilities; children: ReactNode }) {
  return <ViewerContext.Provider value={abilities}>{children}</ViewerContext.Provider>;
}

export function useViewer(): Abilities {
  const value = useContext(ViewerContext);
  if (!value) throw new Error("useViewer() must be used inside <ViewerProvider>");
  return value;
}
