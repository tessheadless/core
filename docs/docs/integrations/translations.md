---
sidebar_position: 1
title: Translations (i18n)
---

# Translations (i18n)

Tess manages the translations of your apps and sites, in the spirit of
[Tolgee](https://tolgee.io): translation **keys**, **languages**, a **review**
step, **import/export** in the usual file formats, **live files** over HTTP and
**delivery** to object storage (Garage, AWS S3, Cloudflare R2, MinIO, Azure Blob).

Everything is in the admin UI under **Translations**, and in the GraphQL API.

## Concepts

| | |
|---|---|
| **Language** | A BCP 47 tag (`en`, `fr-CA`, `zh-Hant`). One of them is the **base** language translators start from. |
| **Key** | A message, e.g. `home.title`, in an optional **namespace** (`common`, `checkout`; empty = default), with a description for translators, tags and a **plural** flag. |
| **Translation** | The text of a key in a language, written in [ICU MessageFormat](https://unicode-org.github.io/icu/userguide/format_parse/messages/): `Hello {name}`, `{count, plural, one {# item} other {# items}}`. |
| **State** | `TRANSLATED` (to review) or `REVIEWED`. A missing translation is *untranslated*. Changing a text sends it back to `TRANSLATED`. |

Translations are stored in the settings store of your database backend, so they
work with every backend (PostgreSQL, MySQL, SQLite, MongoDB, DynamoDB, RocksDB).

## Who may do what

| | Owner / Admin | Contributor | Reviewer | Member |
|---|:-:|:-:|:-:|:-:|
| Read keys, export | ✔ | ✔ | ✔ | ✔ |
| Create keys, edit texts, import | ✔ | ✔ | | |
| Mark texts as reviewed | ✔ | | ✔ | |
| Languages, serving settings, delivery targets, publish | ✔ | | | |

Contributors and Reviewers with a **scope** need `i18n` in it (the scope picker
lists it next to the content types).

## File formats

| Format | Files (namespace `ns`, language `fr-CA`) | Plurals |
|---|---|---|
| `JSON` | `ns/fr-CA.json`, flat `{"home.title": "…"}` | ICU text |
| `JSON_NESTED` | `ns/fr-CA.json`, `{"home": {"title": "…"}}` | ICU text |
| `I18NEXT` | `fr-CA/ns.json` (`translation.json` for the default namespace), `{{var}}` | `key_one`, `key_other`, … |
| `ARB` (Flutter) | `ns/app_fr_CA.arb` | ICU text |
| `XLIFF` 1.2 | `ns/fr-CA.xliff`, source = base language | ICU text |
| `PO` (gettext) | `ns/fr-CA.po`, `msgid` = key | ICU text |
| `PROPERTIES` (Java) | `ns/messages_fr_CA.properties` | ICU text |
| `ANDROID_XML` | `ns/values-fr-rCA/strings.xml` | `<plurals>` |
| `APPLE_STRINGS` | `fr-CA.lproj/ns.strings` (`Localizable.strings` by default) | ICU text |
| `YAML` (Rails) | `ns/fr-CA.yml`, nested under the language | ICU text |
| `CSV` | `ns.csv` (`translations.csv` by default), one column per language | ICU text |

Every format can be **imported**, except YAML. Imports read XLIFF 1.2 and 2.x,
and take the language from the file when it declares one (XLIFF
`target-language`, ARB `@@locale`, PO `Language`). Existing translations are
kept or overridden, and imported texts are `TRANSLATED` or, for reviewers,
`REVIEWED`.

## Live files

Tess serves the current translations at `/i18n/{format}/{file}`, with the same
layout as an export:

```sh
curl https://cms.example.com/i18n/json-nested/fr.json
curl https://cms.example.com/i18n/i18next/fr/common.json
```

Format names in URLs: `json`, `json-nested`, `i18next`, `arb`, `xliff`, `po`,
`properties`, `android`, `apple`, `yaml`, `csv`. Responses carry an `ETag` and
are cached for 60 s. In **Translations → Serving**, Admins choose whether the
files are public (also requires `TESS_PUBLIC_READ`), whether only reviewed
texts are served, and whether missing texts fall back to the base language.

For production traffic, publish to a delivery target behind a CDN instead.

## Delivery targets

A delivery target publishes the exported files to object storage, at
`{pathPrefix}/{format}/{file}`, next to a `manifest.json` listing them:

```json
{
  "publishedAt": "2026-10-20T08:00:00.000Z",
  "baseUrl": "https://cdn.example.com/i18n",
  "languages": ["en", "fr"],
  "files": { "json": ["json/en.json", "json/fr.json"] }
}
```

| Provider | Kind | Endpoint | Region | Notes |
|---|---|---|---|---|
| Garage | `S3` | `http://garage:3900` | `garage` | Path-style URLs. |
| AWS S3 | `S3` | (empty) | your region | Leave both keys empty to use the IAM role of the server. |
| Cloudflare R2 | `S3` | `https://<account-id>.r2.cloudflarestorage.com` | `auto` | R2 API token with Object Read & Write. |
| MinIO | `S3` | `http://minio:9000` | `us-east-1` | Path-style URLs. |
| Azure Blob Storage | `AZURE_BLOB` | (empty; Azurite: `http://127.0.0.1:10000/devstoreaccount1`) | | Storage account + account key (Shared Key), or a container SAS token with create & write permissions. |

Secret keys and SAS tokens are encrypted at rest (`TESS_SECRETS_KEY`) and
never returned by the API. Each target chooses its formats, languages,
namespaces and states (e.g. reviewed only). Publish from the admin UI, with
`publishTranslations`, or turn on **auto-publish**: the target is then published
a few seconds after the last of a burst of changes.

:::note
Android qualifiers for script subtags (`values-b+zh+Hant`) contain `+`, which
S3 targets refuse; use region-only tags for Android on S3, or Azure.
:::

## GraphQL API

```graphql
query {
  translationLanguages { tag name base }
  translationStats { language total translated reviewed }
  translationKeys(filter: { namespace: "common", state: UNTRANSLATED, language: "fr" }, first: 50) {
    total
    items { id name description translations { language text state } }
  }
}

mutation {
  createTranslationKey(input: {
    name: "cart.items", namespace: "shop", plural: true,
    translations: [{ language: "en", text: "{count, plural, one {# item} other {# items}}" }]
  }) { id }
}

mutation($id: ID!) {
  setTranslation(keyId: $id, language: "fr", text: "{count, plural, one {# article} other {# articles}}") { id }
  review: setTranslation(keyId: $id, language: "fr", state: REVIEWED) { id }   # reviewers
}

mutation($file: String!) {
  importTranslations(input: { format: PO, content: $file, language: "de", mode: KEEP }) {
    keysCreated translationsCreated translationsUpdated translationsSkipped
  }
}

query { exportTranslations(input: { format: ANDROID_XML, languages: ["fr"] }) { path content } }
mutation { publishTranslations { name ok files error } }
```

`GET /api/i18n/export?format=PO&languages=en,fr&states=REVIEWED&fallbackToBase=true`
(signed in) downloads an export: one file as is, several as a zip.

Every change emits an `i18n.*` [event](webhooks.md#events), so n8n workflows
can, for example, machine-translate new keys or notify reviewers.
