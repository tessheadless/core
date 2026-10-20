// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { gql } from "./graphql";

export interface Webhook {
  id: string;
  name: string;
  url: string;
  enabled: boolean;
  events: string[];
  contentTypes: string[];
  /** Header names; values are write-only. */
  headers: string[];
  managedBy: string | null;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

export type DeliveryStatus = "RETRYING" | "SUCCESS" | "FAILED";

export interface WebhookDelivery {
  id: string;
  webhookId: string;
  event: string;
  status: DeliveryStatus;
  attempts: number;
  responseStatus: number | null;
  error: string | null;
  durationMs: number;
  createdAt: string;
  updatedAt: string;
}

export interface WebhookInput {
  name: string;
  url: string;
  enabled: boolean;
  events: string[];
  contentTypes: string[];
  /** Omit to keep the current headers (update); values are required when given. */
  headers?: { name: string; value: string }[];
}

const FIELDS = "id name url enabled events contentTypes headers managedBy createdBy createdAt updatedAt";
const DELIVERY_FIELDS = "id webhookId event status attempts responseStatus error durationMs createdAt updatedAt";

export interface WebhooksPageData {
  webhooks: Webhook[];
  webhookEvents: string[];
  contentTypes: { apiId: string }[];
}

export function fetchWebhooksPage() {
  return gql<WebhooksPageData>(`query WebhooksPage {
    webhooks { ${FIELDS} }
    webhookEvents
    contentTypes { apiId }
  }`);
}

export async function createWebhook(input: WebhookInput) {
  const data = await gql<{ createWebhook: { webhook: Webhook; secret: string } }, { input: WebhookInput }>(
    `mutation CreateWebhook($input: WebhookInput!) { createWebhook(input: $input) { secret webhook { ${FIELDS} } } }`,
    { input },
  );
  return data.createWebhook;
}

export async function updateWebhook(id: string, input: Partial<WebhookInput>) {
  const data = await gql<{ updateWebhook: Webhook }, { id: string; input: Partial<WebhookInput> }>(
    `mutation UpdateWebhook($id: ID!, $input: WebhookUpdateInput!) { updateWebhook(id: $id, input: $input) { ${FIELDS} } }`,
    { id, input },
  );
  return data.updateWebhook;
}

export async function deleteWebhook(id: string) {
  await gql<{ deleteWebhook: string }, { id: string }>(`mutation DeleteWebhook($id: ID!) { deleteWebhook(id: $id) }`, {
    id,
  });
}

export async function rotateSecret(id: string) {
  const data = await gql<{ rotateWebhookSecret: string }, { id: string }>(
    `mutation RotateWebhookSecret($id: ID!) { rotateWebhookSecret(id: $id) }`,
    { id },
  );
  return data.rotateWebhookSecret;
}

export async function testWebhook(id: string) {
  const data = await gql<{ testWebhook: WebhookDelivery }, { id: string }>(
    `mutation TestWebhook($id: ID!) { testWebhook(id: $id) { ${DELIVERY_FIELDS} } }`,
    { id },
  );
  return data.testWebhook;
}

export async function fetchDeliveries(id: string) {
  const data = await gql<{ webhookDeliveries: WebhookDelivery[] }, { id: string }>(
    `query WebhookDeliveries($id: ID!) { webhookDeliveries(id: $id, first: 25) { ${DELIVERY_FIELDS} } }`,
    { id },
  );
  return data.webhookDeliveries;
}

/** Groups `entry.create`, `entry.publish`… by subject, for the event picker. */
export function groupEvents(events: string[]): [string, string[]][] {
  const groups = new Map<string, string[]>();
  for (const e of events) {
    const subject = e.slice(0, e.lastIndexOf(".")) || e;
    groups.set(subject, [...(groups.get(subject) ?? []), e]);
  }
  return [...groups.entries()];
}

export const SUBJECT_LABELS: Record<string, string> = {
  entry: "Content entries",
  contentType: "Content models",
  component: "Components",
  "i18n.language": "Translation languages",
  "i18n.key": "Translation keys",
  "i18n.translation": "Translations",
  i18n: "Translation imports & publishing",
  webhook: "Webhook",
};
