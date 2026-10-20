// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { useSyncExternalStore } from "react";

// The bearer token (user session or API token) lives in sessionStorage only: it is
// scoped to the tab and dropped when the tab closes. Never persist it in
// localStorage, URLs or logs.
const STORAGE_KEY = "tess-admin-session";

/**
 * `session`: issued by `login` / `changePassword` (a user account).
 * `token`: an API token or the bootstrap admin token pasted by hand.
 */
export type CredentialKind = "session" | "token";

export interface StoredCredential {
  token: string;
  kind: CredentialKind;
  /** ISO date; null when unknown (pasted tokens). */
  expiresAt: string | null;
}

type Listener = () => void;

const listeners = new Set<Listener>();
// `undefined` = not read from storage yet.
let current: StoredCredential | null | undefined;
let signOutReason: string | null = null;
let flash: string | null = null;

function parse(raw: string | null): StoredCredential | null {
  if (!raw) return null;
  try {
    const value = JSON.parse(raw) as Partial<StoredCredential>;
    if (typeof value.token !== "string" || !value.token) return null;
    return {
      token: value.token,
      kind: value.kind === "session" ? "session" : "token",
      expiresAt: typeof value.expiresAt === "string" ? value.expiresAt : null,
    };
  } catch {
    return null;
  }
}

function readStorage(): StoredCredential | null {
  if (typeof window === "undefined") return null;
  try {
    return parse(window.sessionStorage.getItem(STORAGE_KEY));
  } catch {
    // Storage blocked (privacy mode, sandboxed iframe…): keep the token in memory only.
    return null;
  }
}

function emit() {
  for (const listener of listeners) listener();
}

export function isExpired(credential: StoredCredential, now = Date.now()): boolean {
  if (!credential.expiresAt) return false;
  const at = Date.parse(credential.expiresAt);
  return !Number.isNaN(at) && at <= now;
}

export function getCredential(): StoredCredential | null {
  if (current === undefined) current = readStorage();
  return current;
}

/** The bearer token to send, or null when signed out or expired. */
export function getToken(): string | null {
  const credential = getCredential();
  if (!credential) return null;
  if (isExpired(credential)) {
    clearToken(SESSION_EXPIRED_MESSAGE);
    return null;
  }
  return credential.token;
}

function store(credential: StoredCredential) {
  current = credential;
  signOutReason = null;
  try {
    window.sessionStorage.setItem(STORAGE_KEY, JSON.stringify(credential));
  } catch {
    // Memory-only fallback, see readStorage().
  }
  emit();
}

/** Keep an API / bootstrap token pasted on the sign-in screen. */
export function setToken(token: string) {
  store({ token, kind: "token", expiresAt: null });
}

/** Keep a user session returned by `login` or `changePassword`. */
export function setSession(token: string, expiresAt: string) {
  store({ token, kind: "session", expiresAt });
}

/** Forget the credential. `reason` is shown once on the sign-in screen. */
export function clearToken(reason?: string) {
  current = null;
  flash = null;
  signOutReason = reason ?? null;
  try {
    window.sessionStorage.removeItem(STORAGE_KEY);
  } catch {
    // Nothing stored.
  }
  emit();
}

export function getSignOutReason(): string | null {
  return signOutReason;
}

/** One-off message shown at the top of the app after the next sign-in (e.g. after the first-run setup). */
export function setFlash(message: string) {
  flash = message;
}

/** Pure read (safe in a useState initializer); cleared by clearFlash() or on sign-out. */
export function getFlash(): string | null {
  return flash;
}

export function clearFlash() {
  flash = null;
}

function subscribe(listener: Listener) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useCredential(): StoredCredential | null {
  return useSyncExternalStore(subscribe, getCredential, () => null);
}

export const SESSION_EXPIRED_MESSAGE = "Your session was rejected or has expired. Please sign in again.";
