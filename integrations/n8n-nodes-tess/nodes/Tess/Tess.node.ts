// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import {
  NodeConnectionTypes,
  NodeOperationError,
  type IDataObject,
  type IExecuteFunctions,
  type INodeExecutionData,
  type INodeType,
  type INodeTypeDescription,
} from "n8n-workflow";
import { contentTypeOptions, contentTypes, defaultSelection, formOptions, names, tessGraphql } from "../shared";

const FORMATS = [
  "JSON",
  "JSON_NESTED",
  "I18NEXT",
  "ARB",
  "XLIFF",
  "PO",
  "PROPERTIES",
  "ANDROID_XML",
  "APPLE_STRINGS",
  "YAML",
  "CSV",
];

function json(ctx: IExecuteFunctions, name: string, i: number): IDataObject {
  const raw = ctx.getNodeParameter(name, i, {}) as unknown;
  if (typeof raw === "string") {
    if (!raw.trim()) return {};
    try {
      return JSON.parse(raw) as IDataObject;
    } catch {
      throw new NodeOperationError(ctx.getNode(), `“${name}” is not valid JSON`, { itemIndex: i });
    }
  }
  return (raw as IDataObject) ?? {};
}

export class Tess implements INodeType {
  description: INodeTypeDescription = {
    displayName: "Tess",
    name: "tess",
    icon: "file:tess.svg",
    group: ["transform"],
    version: 1,
    subtitle: '={{$parameter["operation"] + ": " + $parameter["resource"]}}',
    description: "Read and write Tess content and translations through its GraphQL API",
    defaults: { name: "Tess" },
    inputs: [NodeConnectionTypes.Main],
    outputs: [NodeConnectionTypes.Main],
    credentials: [{ name: "tessApi", required: true }],
    properties: [
      {
        displayName: "Resource",
        name: "resource",
        type: "options",
        noDataExpression: true,
        options: [
          { name: "Entry", value: "entry" },
          { name: "Translation", value: "translation" },
          { name: "Form", value: "form" },
          { name: "GraphQL", value: "graphql" },
        ],
        default: "entry",
      },

      // ---- entries ------------------------------------------------------
      {
        displayName: "Operation",
        name: "operation",
        type: "options",
        noDataExpression: true,
        displayOptions: { show: { resource: ["entry"] } },
        options: [
          { name: "Create", value: "create", action: "Create an entry (draft)" },
          { name: "Delete", value: "delete", action: "Delete an entry" },
          { name: "Get", value: "get", action: "Get an entry" },
          { name: "Get Many", value: "getMany", action: "List entries" },
          { name: "Publish", value: "publish", action: "Publish an entry" },
          { name: "Unpublish", value: "unpublish", action: "Unpublish an entry" },
          { name: "Update", value: "update", action: "Update an entry draft" },
        ],
        default: "get",
      },
      {
        displayName: "Content Type Name or ID",
        name: "contentType",
        type: "options",
        typeOptions: { loadOptionsMethod: "getContentTypes" },
        displayOptions: { show: { resource: ["entry"] } },
        default: "",
        required: true,
        description:
          'Choose from the list, or specify an apiId using an <a href="https://docs.n8n.io/code/expressions/">expression</a>. With a Tess Trigger before: {{ $json.contentType }}.',
      },
      {
        displayName: "Entry ID",
        name: "id",
        type: "string",
        displayOptions: { show: { resource: ["entry"], operation: ["get", "update", "delete", "publish", "unpublish"] } },
        default: "={{ $json.data?.entry?.id }}",
        required: true,
        description: "Opaque Tess id (ids expire: fetch them in the same run, e.g. from the trigger payload)",
      },
      {
        displayName: "Data (JSON)",
        name: "data",
        type: "json",
        displayOptions: { show: { resource: ["entry"], operation: ["create", "update"] } },
        default: "{}",
        description: "Field values; relations take entry ids",
      },
      {
        displayName: "Stage",
        name: "stage",
        type: "options",
        displayOptions: { show: { resource: ["entry"], operation: ["get", "getMany"] } },
        options: [
          { name: "Published", value: "PUBLISHED" },
          { name: "Draft", value: "DRAFT" },
        ],
        default: "PUBLISHED",
      },
      {
        displayName: "Filter (JSON)",
        name: "where",
        type: "json",
        displayOptions: { show: { resource: ["entry"], operation: ["getMany"] } },
        default: "{}",
        description: 'A WhereInput, e.g. {"title_contains": "release"}',
      },
      {
        displayName: "Limit",
        name: "limit",
        type: "number",
        typeOptions: { minValue: 1, maxValue: 1000 },
        displayOptions: { show: { resource: ["entry"], operation: ["getMany"] } },
        default: 50,
        description: "Max number of results to return",
      },
      {
        displayName: "Fields",
        name: "fields",
        type: "string",
        displayOptions: { show: { resource: ["entry"], operation: ["create", "update", "get", "getMany", "publish", "unpublish"] } },
        default: "",
        placeholder: "id title author { name }",
        description: "GraphQL selection; empty selects the system fields and every scalar field",
      },

      // ---- translations -------------------------------------------------
      {
        displayName: "Operation",
        name: "operation",
        type: "options",
        noDataExpression: true,
        displayOptions: { show: { resource: ["translation"] } },
        options: [
          { name: "Export", value: "export", action: "Export translation files" },
          { name: "Get Keys", value: "getKeys", action: "List translation keys" },
          { name: "Publish", value: "publish", action: "Publish translations to delivery targets" },
          { name: "Set Text", value: "set", action: "Set a translation text or state" },
        ],
        default: "getKeys",
      },
      {
        displayName: "Search",
        name: "search",
        type: "string",
        displayOptions: { show: { resource: ["translation"], operation: ["getKeys"] } },
        default: "",
      },
      {
        displayName: "Missing In Language",
        name: "missingIn",
        type: "string",
        displayOptions: { show: { resource: ["translation"], operation: ["getKeys"] } },
        default: "",
        placeholder: "fr",
        description: "Only keys without a text in this language (e.g. to machine-translate them)",
      },
      {
        displayName: "Key ID",
        name: "keyId",
        type: "string",
        displayOptions: { show: { resource: ["translation"], operation: ["set"] } },
        default: "",
        required: true,
      },
      {
        displayName: "Language",
        name: "language",
        type: "string",
        displayOptions: { show: { resource: ["translation"], operation: ["set"] } },
        default: "",
        required: true,
        placeholder: "fr",
      },
      {
        displayName: "Text",
        name: "text",
        type: "string",
        typeOptions: { rows: 3 },
        displayOptions: { show: { resource: ["translation"], operation: ["set"] } },
        default: "",
        description: "ICU message; empty keeps the text and only changes the state",
      },
      {
        displayName: "State",
        name: "state",
        type: "options",
        displayOptions: { show: { resource: ["translation"], operation: ["set"] } },
        options: [
          { name: "Unchanged / Default", value: "" },
          { name: "Translated (To Review)", value: "TRANSLATED" },
          { name: "Reviewed", value: "REVIEWED" },
        ],
        default: "",
      },
      {
        displayName: "Format",
        name: "format",
        type: "options",
        displayOptions: { show: { resource: ["translation"], operation: ["export"] } },
        options: FORMATS.map((f) => ({ name: f, value: f })),
        default: "JSON_NESTED",
      },
      {
        displayName: "Languages",
        name: "languages",
        type: "string",
        displayOptions: { show: { resource: ["translation"], operation: ["export"] } },
        default: "",
        placeholder: "en,fr",
        description: "Comma-separated; empty exports every language",
      },
      {
        displayName: "Target ID",
        name: "targetId",
        type: "string",
        displayOptions: { show: { resource: ["translation"], operation: ["publish"] } },
        default: "",
        description: "Delivery target to publish; empty publishes every enabled target",
      },

      // ---- forms --------------------------------------------------------
      {
        displayName: "Operation",
        name: "operation",
        type: "options",
        noDataExpression: true,
        displayOptions: { show: { resource: ["form"] } },
        options: [{ name: "Get Submissions", value: "getSubmissions", action: "List the submissions of a form" }],
        default: "getSubmissions",
      },
      {
        displayName: "Form Name or ID",
        name: "formId",
        type: "options",
        typeOptions: { loadOptionsMethod: "getForms" },
        displayOptions: { show: { resource: ["form"] } },
        default: "",
        required: true,
        description:
          'Choose from the list, or specify an ID using an <a href="https://docs.n8n.io/code/expressions/">expression</a>. New submissions start workflows with the Tess Trigger (form.submitted).',
      },
      {
        displayName: "Limit",
        name: "limit",
        type: "number",
        typeOptions: { minValue: 1, maxValue: 500 },
        displayOptions: { show: { resource: ["form"] } },
        default: 50,
        description: "Max number of results to return, newest first",
      },

      // ---- raw GraphQL --------------------------------------------------
      {
        displayName: "Query",
        name: "query",
        type: "string",
        typeOptions: { rows: 8 },
        displayOptions: { show: { resource: ["graphql"] } },
        default: "{ contentTypes { apiId } }",
        required: true,
        description: "Any query or mutation of the Tess API (the schema is at /graphql/schema.graphql)",
      },
      {
        displayName: "Variables (JSON)",
        name: "variables",
        type: "json",
        displayOptions: { show: { resource: ["graphql"] } },
        default: "{}",
      },
    ],
  };

  methods = { loadOptions: { getContentTypes: contentTypeOptions, getForms: formOptions } };

  async execute(this: IExecuteFunctions): Promise<INodeExecutionData[][]> {
    const items = this.getInputData();
    const out: INodeExecutionData[] = [];
    let types: Awaited<ReturnType<typeof contentTypes>> | null = null;

    for (let i = 0; i < items.length; i++) {
      try {
        const resource = this.getNodeParameter("resource", i) as string;
        const push = (data: IDataObject | IDataObject[]) =>
          out.push(...this.helpers.constructExecutionMetaData(this.helpers.returnJsonArray(data), { itemData: { item: i } }));

        if (resource === "graphql") {
          push(await tessGraphql(this, this.getNodeParameter("query", i) as string, json(this, "variables", i)));
          continue;
        }

        if (resource === "form") {
          const res = await tessGraphql(
            this,
            `query($f: ID!, $n: Int) { formSubmissions(formId: $f, first: $n) { items { id formId data submittedAt source } } }`,
            { f: this.getNodeParameter("formId", i), n: this.getNodeParameter("limit", i, 50) },
          );
          push(((res.formSubmissions as IDataObject).items as IDataObject[]) ?? []);
          continue;
        }

        if (resource === "translation") {
          const op = this.getNodeParameter("operation", i) as string;
          if (op === "getKeys") {
            const missing = (this.getNodeParameter("missingIn", i, "") as string).trim();
            const filter: IDataObject = { search: (this.getNodeParameter("search", i, "") as string) || undefined };
            if (missing) Object.assign(filter, { language: missing, state: "UNTRANSLATED" });
            const res = await tessGraphql(
              this,
              `query($f: TranslationKeyFilter) { translationKeys(filter: $f, first: 1000) {
                items { id name namespace description plural tags translations { language text state } } } }`,
              { f: filter },
            );
            push(((res.translationKeys as IDataObject).items as IDataObject[]) ?? []);
          } else if (op === "set") {
            const text = this.getNodeParameter("text", i, "") as string;
            const state = this.getNodeParameter("state", i, "") as string;
            const res = await tessGraphql(
              this,
              `mutation($k: ID!, $l: String!, $t: String, $s: TranslationState) {
                setTranslation(keyId: $k, language: $l, text: $t, state: $s) { id name translations { language text state } } }`,
              {
                k: this.getNodeParameter("keyId", i),
                l: this.getNodeParameter("language", i),
                t: text || undefined,
                s: state || undefined,
              },
            );
            push(res.setTranslation as IDataObject);
          } else if (op === "export") {
            const languages = (this.getNodeParameter("languages", i, "") as string)
              .split(",")
              .map((l) => l.trim())
              .filter(Boolean);
            const res = await tessGraphql(
              this,
              `query($i: TranslationExportInput!) { exportTranslations(input: $i) { path contentType content } }`,
              { i: { format: this.getNodeParameter("format", i), languages } },
            );
            push(res.exportTranslations as IDataObject[]);
          } else {
            const targetId = (this.getNodeParameter("targetId", i, "") as string) || undefined;
            const res = await tessGraphql(
              this,
              `mutation($t: ID) { publishTranslations(targetId: $t) { targetId name ok files error publishedAt } }`,
              { t: targetId },
            );
            push(res.publishTranslations as IDataObject[]);
          }
          continue;
        }

        // Entries.
        types ??= await contentTypes(this);
        const apiId = this.getNodeParameter("contentType", i) as string;
        const type = types.find((t) => t.apiId === apiId);
        if (!type) throw new NodeOperationError(this.getNode(), `Unknown content type “${apiId}”`, { itemIndex: i });
        const n = names(type);
        const op = this.getNodeParameter("operation", i) as string;
        const selection = ((this.getNodeParameter("fields", i, "") as string) || "").trim() || defaultSelection(type);
        const id = () => this.getNodeParameter("id", i) as string;
        const stage = () => this.getNodeParameter("stage", i, "PUBLISHED") as string;

        let res: IDataObject;
        switch (op) {
          case "get":
            res = await tessGraphql(this, `query($id: ID!, $s: Stage) { r: ${n.single}(id: $id, stage: $s) { ${selection} } }`, {
              id: id(),
              s: stage(),
            });
            break;
          case "getMany": {
            res = await tessGraphql(
              this,
              `query($w: ${n.whereInput}, $f: Int, $s: Stage) { r: ${n.list}(where: $w, first: $f, stage: $s) { total items { ${selection} } } }`,
              { w: json(this, "where", i), f: this.getNodeParameter("limit", i, 50), s: stage() },
            );
            push(((res.r as IDataObject).items as IDataObject[]) ?? []);
            continue;
          }
          case "create":
            res = await tessGraphql(this, `mutation($d: ${n.createInput}!) { r: ${n.create}(data: $d) { ${selection} } }`, {
              d: json(this, "data", i),
            });
            break;
          case "update":
            res = await tessGraphql(
              this,
              `mutation($id: ID!, $d: ${n.updateInput}!) { r: ${n.update}(id: $id, data: $d) { ${selection} } }`,
              { id: id(), d: json(this, "data", i) },
            );
            break;
          case "delete":
            res = await tessGraphql(this, `mutation($id: ID!) { r: ${n.delete}(id: $id) }`, { id: id() });
            res = { r: { id: res.r, deleted: true } };
            break;
          case "publish":
          case "unpublish":
            res = await tessGraphql(this, `mutation($id: ID!) { r: ${n[op]}(id: $id) { ${selection} } }`, { id: id() });
            break;
          default:
            throw new NodeOperationError(this.getNode(), `Unknown operation “${op}”`, { itemIndex: i });
        }
        push((res.r as IDataObject) ?? {});
      } catch (error) {
        if (this.continueOnFail()) {
          out.push({ json: { error: (error as Error).message }, pairedItem: { item: i } });
          continue;
        }
        throw error;
      }
    }
    return [out];
  }
}

