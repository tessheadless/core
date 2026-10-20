---
sidebar_position: 2
title: Community & Enterprise
---

# Community and Enterprise editions

Tess is distributed under a **hybrid license** with two editions.

| | **Community** | **Enterprise** |
|---|---|---|
| Intended use | Personal use | Commercial use |
| Core CMS: content models, GraphQL API, draft/publish, review workflow, users and roles, media, search, plugins, every database backend, Docker and AWS Lambda | ✔ | ✔ |
| Advanced features | | ✔ |
| Official plugins maintained by the Tess Dev Team | | ✔ |
| Hosting solutions for Tess | | ✔ |

:::info Which edition do I need?
If you use Tess for a business or any commercial purpose, you need the
**Enterprise** edition. The **Community** edition is for personal use.
The terms in the project's `LICENSE.md` are authoritative; this page only
summarizes them.
:::

## Community edition

The Community edition is the full Tess engine documented on this site. Use it
to learn Tess, build personal projects, and develop and test your own plugins.

## Enterprise edition

The Enterprise edition is for organizations using Tess commercially. It adds:

- **Advanced features**, built on top of the Community core. Single sign-on
  (OIDC) is the first one planned; today, users sign in with email and
  password.
- **Official plugins** developed and maintained by the Tess Dev Team.
- **Hosting solutions**, if you'd rather not operate Tess yourself.

:::note
The Enterprise offer is being prepared. Features listed as planned aren't
available yet, and this documentation only describes what exists in the code
today.
:::

For Enterprise licensing and hosting, see
<a href="/">tessheadless.com</a>.

## Plugins and editions

You can write and run your own plugins with either edition (see
[Plugins](plugins/overview.md)). Official Enterprise plugins are distributed
as WebAssembly modules and installed exactly like your own.
