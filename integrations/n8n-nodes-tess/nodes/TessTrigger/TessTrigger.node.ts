// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import {
  NodeConnectionTypes,
  type IDataObject,
  type IHookFunctions,
  type INodeType,
  type INodeTypeDescription,
  type IWebhookFunctions,
  type IWebhookResponseData,
} from "n8n-workflow";
import { contentTypeOptions, eventOptions, rewriteOrigin, tessGraphql, verifySignature } from "../shared";

interface StaticData {
  webhookId?: string;
  secret?: string;
}

/**
 * Starts a workflow when Tess emits an event. Activating the workflow
 * registers a Tess webhook pointing at this node; deactivating removes it.
 */
export class TessTrigger implements INodeType {
  description: INodeTypeDescription = {
    displayName: "Tess Trigger",
    name: "tessTrigger",
    icon: "file:tess.svg",
    group: ["trigger"],
    version: 1,
    subtitle: '={{$parameter["events"].join(", ")}}',
    description: "Starts the workflow when content, models or translations change in Tess",
    defaults: { name: "Tess Trigger" },
    inputs: [],
    outputs: [NodeConnectionTypes.Main],
    credentials: [{ name: "tessApi", required: true }],
    webhooks: [{ name: "default", httpMethod: "POST", responseMode: "onReceived", path: "webhook" }],
    properties: [
      {
        displayName: "Events",
        name: "events",
        type: "multiOptions",
        typeOptions: { loadOptionsMethod: "getEvents" },
        required: true,
        default: ["entry.publish"],
        description:
          "Events that start the workflow: entry.create/update/delete/publish/…, contentType.*, component.*, i18n.*",
      },
      {
        displayName: "Content Types",
        name: "contentTypes",
        type: "multiOptions",
        typeOptions: { loadOptionsMethod: "getContentTypes" },
        default: [],
        description: "Only events about these content types (none = all). Component and translation events always pass.",
      },
      {
        displayName: "Verify Signature",
        name: "verifySignature",
        type: "boolean",
        default: true,
        description: "Whether to reject requests whose X-Tess-Signature does not match the webhook secret",
      },
      {
        displayName: "n8n URL for Tess",
        name: "tessReachUrl",
        type: "string",
        default: "",
        placeholder: "http://n8n:5678",
        description:
          "Origin Tess must use to reach this n8n instance, when it differs from the public one (e.g. inside Docker Compose). Leave empty to use n8n's webhook URL.",
      },
    ],
  };

  methods = {
    loadOptions: { getEvents: eventOptions, getContentTypes: contentTypeOptions },
  };

  webhookMethods = {
    default: {
      async checkExists(this: IHookFunctions): Promise<boolean> {
        const data = this.getWorkflowStaticData("node") as StaticData;
        if (!data.webhookId) return false;
        const res = await tessGraphql(this, "query($id: ID!) { webhook(id: $id) { id url } }", { id: data.webhookId });
        const hook = res.webhook as { url: string } | null;
        if (!hook) {
          delete data.webhookId;
          delete data.secret;
          return false;
        }
        return true;
      },

      async create(this: IHookFunctions): Promise<boolean> {
        const publicUrl = this.getNodeWebhookUrl("default");
        if (!publicUrl) return false;
        const url = rewriteOrigin(publicUrl, this.getNodeParameter("tessReachUrl", "") as string);
        const workflow = this.getWorkflow();
        const input = {
          name: `n8n · ${workflow.name ?? workflow.id}`.slice(0, 100),
          url,
          enabled: true,
          events: this.getNodeParameter("events", []) as string[],
          contentTypes: this.getNodeParameter("contentTypes", []) as string[],
          managedBy: "n8n",
        };
        const res = await tessGraphql(
          this,
          "mutation($input: WebhookInput!) { createWebhook(input: $input) { secret webhook { id } } }",
          { input },
        );
        const created = res.createWebhook as { secret: string; webhook: { id: string } };
        const data = this.getWorkflowStaticData("node") as StaticData;
        data.webhookId = created.webhook.id;
        data.secret = created.secret;
        return true;
      },

      async delete(this: IHookFunctions): Promise<boolean> {
        const data = this.getWorkflowStaticData("node") as StaticData;
        if (data.webhookId) {
          try {
            await tessGraphql(this, "mutation($id: ID!) { deleteWebhook(id: $id) }", { id: data.webhookId });
          } catch {
            // Already removed in Tess.
          }
        }
        delete data.webhookId;
        delete data.secret;
        return true;
      },
    },
  };

  async webhook(this: IWebhookFunctions): Promise<IWebhookResponseData> {
    const req = this.getRequestObject();
    const body = this.getBodyData() as IDataObject;
    if (this.getNodeParameter("verifySignature", true) as boolean) {
      const { secret } = this.getWorkflowStaticData("node") as StaticData;
      const raw = (req as unknown as { rawBody?: Buffer }).rawBody ?? Buffer.from(JSON.stringify(body));
      const header = req.headers["x-tess-signature"];
      if (!secret || !verifySignature(secret, Array.isArray(header) ? header[0] : header, raw)) {
        this.getResponseObject().status(401).json({ error: "invalid Tess signature" });
        return { noWebhookResponse: true };
      }
    }
    return { workflowData: [this.helpers.returnJsonArray(body)] };
  }
}
