// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import "./tailwind.css";
import { useEffect, useState, type ReactNode } from "react";
import logoUrl from "../assets/logo.svg";
import { NavLink, withBase } from "../components/Link";
import { RoleBadge } from "../components/RoleBadge";
import { SignIn } from "../components/SignIn";
import { ToastProvider } from "../components/Toasts";
import { fetchMe } from "../lib/accounts";
import {
  clearToken,
  SESSION_EXPIRED_MESSAGE,
  clearFlash,
  getFlash,
  useCredential,
  type StoredCredential,
} from "../lib/auth";
import { ApiError, describeError, gql } from "../lib/graphql";
import { abilitiesFor, useViewer, ViewerProvider, type Abilities, type Viewer } from "../lib/viewer";

export default function Layout({ children }: { children: ReactNode }) {
  const credential = useCredential();
  return (
    <ToastProvider>{credential ? <SignedIn credential={credential}>{children}</SignedIn> : <SignIn />}</ToastProvider>
  );
}

/** Who is behind the credential: `me` for user sessions; tokens are probed with an admin-only query. */
async function loadViewer(): Promise<Viewer> {
  const user = await fetchMe();
  if (user) return { kind: "user", user };
  try {
    // `users` is admin-only (`contentTypes` is readable by every signed-in caller).
    await gql<{ users: { id: string }[] }>("query TokenAdminProbe { users { id } }");
    return { kind: "token", admin: true };
  } catch (e) {
    if (e instanceof ApiError && e.code === "FORBIDDEN") return { kind: "token", admin: false };
    throw e;
  }
}

// setTimeout overflows above ~24.8 days.
const MAX_TIMEOUT = 2 ** 31 - 1;

function SignedIn({ credential, children }: { credential: StoredCredential; children: ReactNode }) {
  const [abilities, setAbilities] = useState<Abilities | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const isSession = credential.kind === "session";

  useEffect(() => {
    let cancelled = false;
    loadViewer().then(
      (viewer) => {
        if (!cancelled) setAbilities(abilitiesFor(viewer));
      },
      (e: unknown) => {
        if (cancelled) return;
        // A user session that may not even read `me` is unusable: sign out (401s already did).
        if (e instanceof ApiError && e.code === "FORBIDDEN") clearToken(SESSION_EXPIRED_MESSAGE);
        else setError(describeError(e));
      },
    );
    return () => {
      cancelled = true;
    };
    // Reload when switching between a user session and a token (not on password change).
  }, [attempt, isSession]);

  // Sign out when the session expires, even if the tab stays idle.
  useEffect(() => {
    if (!credential.expiresAt) return;
    const delay = Date.parse(credential.expiresAt) - Date.now();
    if (Number.isNaN(delay)) return;
    if (delay <= 0) {
      clearToken(SESSION_EXPIRED_MESSAGE);
      return;
    }
    const timer = window.setTimeout(() => clearToken(SESSION_EXPIRED_MESSAGE), Math.min(delay, MAX_TIMEOUT));
    return () => window.clearTimeout(timer);
  }, [credential.expiresAt]);

  if (error) {
    return (
      <main className="bg-base-200 flex min-h-screen items-center justify-center p-4">
        <div role="alert" className="alert alert-error max-w-lg">
          <span>Could not load your account: {error}</span>
          <div className="flex gap-2">
            <button
              type="button"
              className="btn btn-sm"
              onClick={() => {
                setError(null);
                setAttempt((n) => n + 1);
              }}
            >
              Retry
            </button>
            <button type="button" className="btn btn-sm btn-ghost" onClick={() => clearToken()}>
              Sign out
            </button>
          </div>
        </div>
      </main>
    );
  }
  if (!abilities) {
    return (
      <main className="flex min-h-screen items-center justify-center">
        <span className="loading loading-spinner loading-lg" aria-label="Loading your account" />
      </main>
    );
  }
  return (
    <ViewerProvider abilities={abilities}>
      <Shell>{children}</Shell>
    </ViewerProvider>
  );
}

interface NavItem {
  href: string;
  label: string;
}

function navItems(a: Abilities): NavItem[] {
  const items: NavItem[] = [
    { href: "/", label: "Dashboard" },
    { href: "/content", label: "Content" },
    { href: "/translations", label: "Translations" },
  ];
  if (a.canSeeReviewQueue)
    items.push({ href: "/review", label: a.role === "CONTRIBUTOR" ? "Changes requested" : "Review queue" });
  if (a.isAdmin || a.role === "CONTRIBUTOR") items.push({ href: "/workflows", label: "Workflows" });
  if (a.isAdmin) {
    items.push(
      { href: "/content-models", label: "Content models" },
      { href: "/components", label: "Components" },
      { href: "/users", label: "Users" },
      { href: "/api-tokens", label: "API tokens" },
      { href: "/search", label: "Search" },
      { href: "/webhooks", label: "Webhooks" },
      { href: "/plugins", label: "Plugins" },
    );
  }
  if (a.viewer.kind === "user") items.push({ href: "/account", label: "My account" });
  return items;
}

function Shell({ children }: { children: ReactNode }) {
  const abilities = useViewer();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [flash, setFlashMessage] = useState<string | null>(getFlash);
  const closeDrawer = () => setDrawerOpen(false);
  const { viewer } = abilities;

  return (
    <div className="drawer lg:drawer-open">
      <input
        id="nav-drawer"
        type="checkbox"
        className="drawer-toggle"
        checked={drawerOpen}
        onChange={(e) => setDrawerOpen(e.target.checked)}
        aria-label="Toggle navigation"
      />
      <div className="drawer-content flex min-h-screen flex-col">
        <header className="navbar bg-base-200 lg:hidden">
          <label htmlFor="nav-drawer" className="btn btn-square btn-ghost" aria-label="Open navigation">
            <MenuIcon />
          </label>
          <span className="px-2 text-lg font-semibold">Tess Admin</span>
        </header>
        <main className="w-full max-w-7xl flex-1 p-4 lg:p-8">
          {flash && (
            <div role="alert" className="alert alert-warning mb-6">
              <span>{flash}</span>
              <button
                type="button"
                className="btn btn-sm btn-ghost"
                onClick={() => {
                  clearFlash();
                  setFlashMessage(null);
                }}
              >
                Dismiss
              </button>
            </div>
          )}
          {children}
        </main>
      </div>
      <div className="drawer-side z-40">
        <label htmlFor="nav-drawer" className="drawer-overlay" aria-label="Close navigation" />
        <aside className="bg-base-200 flex min-h-full w-64 flex-col">
          <a href={withBase("/")} className="flex items-center gap-3 p-4 text-lg font-semibold" onClick={closeDrawer}>
            <img src={logoUrl} width={32} height={32} alt="" />
            Tess Admin
          </a>
          <nav aria-label="Main" className="flex-1">
            <ul className="menu w-full">
              {navItems(abilities).map((item) => (
                <li key={item.href}>
                  <NavLink href={item.href} onClick={closeDrawer}>
                    {item.label}
                  </NavLink>
                </li>
              ))}
            </ul>
          </nav>
          <div className="border-base-300 flex flex-col gap-3 border-t p-4">
            {viewer.kind === "user" ? (
              <div className="min-w-0">
                <div className="truncate font-semibold" title={viewer.user.name}>
                  {viewer.user.name}
                </div>
                <div className="text-base-content/70 truncate text-xs" title={viewer.user.email}>
                  {viewer.user.email}
                </div>
                <div className="mt-1">
                  <RoleBadge role={viewer.user.role} />
                </div>
              </div>
            ) : (
              <div>
                <div className="font-semibold">API token</div>
                <span className="badge badge-sm badge-neutral">{viewer.admin ? "Admin access" : "Limited access"}</span>
              </div>
            )}
            <button type="button" className="btn btn-outline btn-sm w-full" onClick={() => clearToken()}>
              Sign out
            </button>
          </div>
        </aside>
      </div>
    </div>
  );
}

function MenuIcon() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" className="size-6" aria-hidden="true">
      <path stroke="currentColor" strokeLinecap="round" strokeWidth="2" d="M4 6h16M4 12h16M4 18h16" />
    </svg>
  );
}
