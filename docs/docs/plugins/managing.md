---
sidebar_position: 7
title: Managing plugins
---

# Managing plugins

Plugins are managed by **Admins and Owners**, in **Admin → Plugins** or
through the API.

## Install or replace a module

Plugin names are 2 to 48 characters: lowercase letters, digits and `-`,
starting with a letter.

```bash
curl -s -X POST https://cms.example.com/api/plugins/slugify \
  -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/wasm' \
  --data-binary @slugify_plugin.wasm
```

The response (`201`) is the plugin as JSON. Small modules can also be sent
through GraphQL:

```graphql
mutation($wasm: String!) {
  installPlugin(name: "slugify", wasmBase64: $wasm) { name version exports enabled }
}
```

At install time, Tess checks the module:

- it must be WebAssembly;
- it must export `before_write` and/or `after_write`;
- its `manifest`, if exported, is read for the version and description.

A **new** plugin is installed **disabled**. Uploading a module under an
existing name **replaces the module and keeps its settings**.

## Configure and enable

```graphql
mutation {
  updatePlugin(name: "slugify", input: {
    enabled: true
    contentTypes: ["Post", "Page"]   # empty list = all content types
    config: { forbiddenWords: ["casino"] }
    allowedHosts: []                 # e.g. ["hooks.example.com", "*.example.com"]
    timeoutMs: 1000
    memoryMaxPages: 256
    failOpen: false
  }) { enabled loaded lastError }
}
```

Omitted fields are unchanged. Limits are bounded by the operator's maxima,
which `pluginLimits { maxModuleBytes maxTimeoutMs maxMemoryPages allowHttp }`
returns.

## Inspect and remove

```graphql
{
  plugins {
    name version description enabled loaded lastError
    exports contentTypes allowedHosts config
    timeoutMs memoryMaxPages failOpen sha256 size installedAt updatedAt
  }
}
```

- `loaded`: the plugin is running on the instance that answered.
- `lastError`: the last failure seen on that instance.

```graphql
mutation { uninstallPlugin(name: "slugify") }
```

Uninstalling removes the settings and deletes the module from the media
store.
