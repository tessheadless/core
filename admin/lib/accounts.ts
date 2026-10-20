// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { gql } from "./graphql";

export const ROLES = ["OWNER", "ADMIN", "CONTRIBUTOR", "REVIEWER", "MEMBER"] as const;
export type Role = (typeof ROLES)[number];

export const ROLE_LABEL: Record<Role, string> = {
  OWNER: "Owner",
  ADMIN: "Admin",
  CONTRIBUTOR: "Contributor",
  REVIEWER: "Reviewer",
  MEMBER: "Member",
};

export const ROLE_DESCRIPTION: Record<Role, string> = {
  OWNER: "Full access, including managing other Owners.",
  ADMIN: "Manages users, API tokens, content models and plugins; publishes anything.",
  CONTRIBUTOR: "Creates and edits drafts, submits them for review, uploads media.",
  REVIEWER: "Approves or requests changes on content in review. Cannot edit.",
  MEMBER: "Read-only access, drafts included.",
};

/** Roles whose `scope` (content type apiIds) restricts what they can touch. */
export function roleUsesScope(role: Role): boolean {
  return role === "CONTRIBUTOR" || role === "REVIEWER";
}

export interface User {
  id: string;
  email: string;
  name: string;
  role: Role;
  scope: string[];
  disabled: boolean;
  createdAt: string;
  lastLoginAt: string | null;
}

export interface ApiToken {
  id: string;
  name: string;
  role: Role;
  scope: string[];
  createdBy: string;
  createdAt: string;
  expiresAt: string | null;
}

export interface Session {
  token: string;
  expiresAt: string;
  user: User;
}

const USER_FIELDS = "id email name role scope disabled createdAt lastLoginAt";
const TOKEN_FIELDS = "id name role scope createdBy createdAt expiresAt";

// --- Public / authentication -------------------------------------------------

export async function fetchSetupRequired(): Promise<boolean> {
  const data = await gql<{ setupRequired: boolean }>("query SetupRequired { setupRequired }", undefined, {
    anonymous: true,
  });
  return data.setupRequired;
}

export async function login(email: string, password: string): Promise<Session> {
  const data = await gql<{ login: Session }, { email: string; password: string }>(
    `mutation Login($email: String!, $password: String!) {
      login(email: $email, password: $password) { token expiresAt user { ${USER_FIELDS} } }
    }`,
    { email, password },
    { anonymous: true },
  );
  return data.login;
}

export async function fetchMe(): Promise<User | null> {
  const data = await gql<{ me: User | null }>(`query Me { me { ${USER_FIELDS} } }`);
  return data.me;
}

/** Returns a new session; every previous token of the user is revoked. */
export async function changePassword(currentPassword: string, newPassword: string): Promise<Session> {
  const data = await gql<{ changePassword: Session }, { currentPassword: string; newPassword: string }>(
    `mutation ChangePassword($currentPassword: String!, $newPassword: String!) {
      changePassword(currentPassword: $currentPassword, newPassword: $newPassword) {
        token expiresAt user { ${USER_FIELDS} }
      }
    }`,
    { currentPassword, newPassword },
  );
  return data.changePassword;
}

export async function signOutEverywhere(): Promise<boolean> {
  const data = await gql<{ signOutEverywhere: boolean }>("mutation SignOutEverywhere { signOutEverywhere }");
  return data.signOutEverywhere;
}

// --- Users (admin) -----------------------------------------------------------

export interface UserInput {
  email: string;
  name: string;
  role: Role;
  scope: string[];
  password: string;
}

/** Omitted fields are left unchanged. */
export interface UserUpdateInput {
  name?: string;
  role?: Role;
  scope?: string[];
  disabled?: boolean;
  password?: string;
}

export interface UsersPageData {
  users: User[];
  contentTypes: { apiId: string }[];
}

export function fetchUsersPage() {
  return gql<UsersPageData>(`query UsersPage { users { ${USER_FIELDS} } contentTypes { apiId } }`);
}

/** `token` lets the first-run setup call this with the bootstrap admin token. */
export async function createUser(input: UserInput, token?: string): Promise<User> {
  const data = await gql<{ createUser: User }, { input: UserInput }>(
    `mutation CreateUser($input: UserInput!) { createUser(input: $input) { ${USER_FIELDS} } }`,
    { input },
    token === undefined ? {} : { token },
  );
  return data.createUser;
}

export async function updateUser(id: string, input: UserUpdateInput): Promise<User> {
  const data = await gql<{ updateUser: User }, { id: string; input: UserUpdateInput }>(
    `mutation UpdateUser($id: ID!, $input: UserUpdateInput!) { updateUser(id: $id, input: $input) { ${USER_FIELDS} } }`,
    { id, input },
  );
  return data.updateUser;
}

export async function deleteUser(id: string): Promise<string> {
  const data = await gql<{ deleteUser: string }, { id: string }>(
    "mutation DeleteUser($id: ID!) { deleteUser(id: $id) }",
    { id },
  );
  return data.deleteUser;
}

// --- API tokens (admin) ------------------------------------------------------

export interface TokensPageData {
  apiTokens: ApiToken[];
  contentTypes: { apiId: string }[];
}

export function fetchTokensPage() {
  return gql<TokensPageData>(`query ApiTokensPage { apiTokens { ${TOKEN_FIELDS} } contentTypes { apiId } }`);
}

export type CreateApiTokenVars = {
  name: string;
  role: Role;
  scope: string[];
  /** null = never expires. */
  expiresInDays: number | null;
};

export interface CreatedApiToken {
  token: string;
  apiToken: ApiToken;
}

export async function createApiToken(vars: CreateApiTokenVars): Promise<CreatedApiToken> {
  // "Never expires" leaves the argument out of the document: the server rejects a null
  // `expiresInDays`, even through an unset variable.
  const { expiresInDays, ...rest } = vars;
  const withExpiry = expiresInDays !== null;
  const data = await gql<
    { createApiToken: CreatedApiToken },
    Omit<CreateApiTokenVars, "expiresInDays"> & { expiresInDays?: number }
  >(
    `mutation CreateApiToken($name: String!, $role: Role!, $scope: [String!]${withExpiry ? ", $expiresInDays: Int!" : ""}) {
      createApiToken(name: $name, role: $role, scope: $scope${withExpiry ? ", expiresInDays: $expiresInDays" : ""}) {
        token apiToken { ${TOKEN_FIELDS} }
      }
    }`,
    withExpiry ? { ...rest, expiresInDays } : rest,
  );
  return data.createApiToken;
}

export async function revokeApiToken(id: string): Promise<string> {
  const data = await gql<{ revokeApiToken: string }, { id: string }>(
    "mutation RevokeApiToken($id: String!) { revokeApiToken(id: $id) }",
    { id },
  );
  return data.revokeApiToken;
}

export const MIN_PASSWORD_LENGTH = 12;

/** Client-side check mirroring the server rule; returns an error message or null. */
export function passwordProblem(password: string): string | null {
  if (Array.from(password).length < MIN_PASSWORD_LENGTH) return `Use at least ${MIN_PASSWORD_LENGTH} characters.`;
  return null;
}

export const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
