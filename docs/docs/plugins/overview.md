---
sidebar_position: 1
title: Overview
---

# Plugins

Plugins add your own logic to Tess **without rebuilding or redeploying it**.
A plugin is a **WebAssembly module** built with an
[Extism PDK](https://extism.org/docs/concepts/pdk), in Rust, Go, JavaScript
and other languages. An admin uploads it, enables it, scopes it and
configures it at runtime.

## What plugins can do

| Hook | Runs | Can |
|---|---|---|
| `before_write` | before a change is saved | **transform** the data (fill a slug, normalize text, compute a field), or **reject** the change with a message |
| `after_write` | after a change is saved | **react**: send a webhook, notify a chat, trigger a static-site rebuild |

Examples in the repository (`examples/plugins/`):

- **`rust-slugify`**: fills `slug` from `title`, rejects forbidden words.
  See the [Rust tutorial](tutorial-rust.md).
- **`rust-webhook`**: POSTs a notification when entries are published.
- **`go-reading-time`**: computes a reading time. See the
  [Go tutorial](tutorial-go.md).
- **`js-moderation`**: trims text, checks required fields before publishing.
  See the [JavaScript tutorial](tutorial-javascript.md).

## Why WebAssembly

- **Sandboxed**: no filesystem, no environment, no network unless allowed,
  plus a timeout and a memory cap. See [Sandbox](sandbox.md).
- **Any language** with an Extism PDK.
- **Works everywhere Tess runs**, including AWS Lambda.
- **Hot-loaded**: enable, disable or replace a plugin while Tess runs.

Built-in features (search indexing, media cleanup) use the same hook
system in the core.

## Lifecycle

1. **Build** a `.wasm` module ([Rust](tutorial-rust.md), [Go](tutorial-go.md),
   [JavaScript](tutorial-javascript.md)).
2. **Upload** it in **Admin → Plugins**, or with the API. It is installed
   **disabled**.
3. **Configure**: content types, JSON config, allowed hosts, timeout,
   memory, fail-open.
4. **Enable**. It runs immediately on the instance that handled the change,
   and on the others within `TESS_SCHEMA_REFRESH_SECS`.

Read [the contract](contract.md) before writing your first plugin.

:::info Official plugins
The [Enterprise edition](../editions.md) includes plugins maintained by the
Tess Dev Team. They're installed and managed exactly like your own.
:::
