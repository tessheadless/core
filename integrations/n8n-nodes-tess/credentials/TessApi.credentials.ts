// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import type { IAuthenticateGeneric, ICredentialTestRequest, ICredentialType, INodeProperties } from "n8n-workflow";

export class TessApi implements ICredentialType {
  name = "tessApi";
  displayName = "Tess API";
  documentationUrl = "https://tessheadless.com/docs/";
  icon = "file:tess.svg" as const;

  properties: INodeProperties[] = [
    {
      displayName: "Tess URL",
      name: "baseUrl",
      type: "string",
      default: "http://tess:8080",
      placeholder: "https://cms.example.com",
      description: "Base URL of the Tess server as n8n reaches it (in Docker Compose: http://tess:8080)",
    },
    {
      displayName: "API Token",
      name: "apiToken",
      type: "string",
      typeOptions: { password: true },
      default: "",
      description:
        "A Tess API token (Admin → API tokens). The Tess Trigger node needs the Admin role to register its webhook.",
    },
  ];

  authenticate: IAuthenticateGeneric = {
    type: "generic",
    properties: { headers: { Authorization: "=Bearer {{$credentials.apiToken}}" } },
  };

  test: ICredentialTestRequest = {
    request: {
      baseURL: "={{$credentials.baseUrl.replace(/\\/$/, '')}}",
      url: "/graphql",
      method: "POST",
      body: { query: "{ contentTypes { apiId } }" },
      json: true,
    },
    rules: [{ type: "responseSuccessBody", properties: { key: "errors", value: undefined, message: "" } }],
  };
}
