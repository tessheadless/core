// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Translations API: languages, keys, review states, import/export and
//! delivery targets. Reads need any signed-in caller; see `tess_i18n` for
//! who may edit, review and administer.

use std::sync::Arc;

use async_graphql::dynamic::{Field, FieldFuture, FieldValue, InputObject, InputValue, ResolverContext, TypeRef};
use serde_json::{Value, json};
use tess_core::CmsError;
use tess_i18n::{
    AzureAuth, DeliveryInput, DeliveryTarget, DeliveryUpdate, ExportOptions, Format, I18nService, I18nSettings,
    ImportInput, ImportMode, KeyFilter, KeyInput, KeyUpdate, StorageKind, TranslationKey, TranslationState,
};

use crate::jsonobj::{enum_type, enumerated, json_object, object, plain, value, values};
use crate::schema::{GqlResult, TypeDef, admin_require, cms_err, json_arg, require_write};

fn service<'a>(ctx: &ResolverContext<'a>) -> GqlResult<&'a Arc<I18nService>> {
    ctx.data::<Arc<I18nService>>()
}

fn key_json(k: &TranslationKey) -> Value {
    let translations: Vec<Value> = k
        .translations
        .iter()
        .map(|(lang, t)| {
            json!({ "language": lang, "text": t.text, "state": t.state, "updatedAt": t.updated_at, "updatedBy": t.updated_by })
        })
        .collect();
    json!({
        "id": k.id(), "name": k.name, "namespace": k.namespace, "description": k.description, "tags": k.tags,
        "plural": k.plural, "translations": translations, "createdAt": k.created_at, "updatedAt": k.updated_at,
    })
}

fn target_json(t: &DeliveryTarget) -> Value {
    let mut v = serde_json::to_value(t).unwrap_or_default();
    if let Value::Object(m) = &mut v {
        m.remove("secret");
        m.insert("hasSecret".into(), json!(t.has_secret()));
    }
    v
}

fn bad(msg: impl Into<String>) -> async_graphql::Error {
    cms_err(CmsError::validation(msg.into()))
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(String::from)
}

fn b(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(Value::as_bool)
}

fn list(v: &Value, k: &str) -> Option<Vec<String>> {
    v.get(k)?.as_array().map(|xs| xs.iter().filter_map(|x| x.as_str().map(String::from)).collect())
}

/// Present key: `Some(None)` for an explicit null.
fn nullable(v: &Value, k: &str) -> Option<Option<String>> {
    v.get(k).map(|x| x.as_str().map(String::from))
}

fn format_of(v: &Value, k: &str) -> GqlResult<Option<Format>> {
    s(v, k).map(|f| Format::parse(&f).ok_or_else(|| bad(format!("unknown format `{f}`")))).transpose()
}

fn formats_of(v: &Value, k: &str) -> GqlResult<Option<Vec<Format>>> {
    list(v, k)
        .map(|xs| xs.iter().map(|f| Format::parse(f).ok_or_else(|| bad(format!("unknown format `{f}`")))).collect())
        .transpose()
}

fn states_of(v: &Value, k: &str) -> Option<Vec<TranslationState>> {
    list(v, k).map(|xs| xs.iter().filter_map(|x| TranslationState::parse(x)).collect())
}

fn kind_of(v: &Value) -> Option<StorageKind> {
    match s(v, "kind").as_deref() {
        Some("AZURE_BLOB") => Some(StorageKind::AzureBlob),
        Some("S3") => Some(StorageKind::S3),
        _ => None,
    }
}

fn azure_auth_of(v: &Value) -> Option<AzureAuth> {
    match s(v, "azureAuth").as_deref() {
        Some("SAS") => Some(AzureAuth::Sas),
        Some("SHARED_KEY") => Some(AzureAuth::SharedKey),
        _ => None,
    }
}

fn types() -> Vec<TypeDef> {
    use TypeRef as T;
    let nn = T::named_nn;
    let strs = || T::named_nn_list_nn(T::STRING);
    let objects = vec![
        json_object(
            "TranslationLanguage",
            "A language translations are written in.",
            vec![
                plain("tag", nn(T::STRING)).doc("BCP 47 tag, e.g. `en`, `fr-CA`."),
                plain("name", nn(T::STRING)),
                plain("base", nn(T::BOOLEAN)).doc("The source language translators work from."),
            ],
        ),
        json_object(
            "Translation",
            "The text of a key in one language.",
            vec![
                plain("language", nn(T::STRING)),
                plain("text", nn(T::STRING)).doc("ICU MessageFormat."),
                enumerated("state", nn("TranslationState")),
                plain("updatedAt", nn("DateTime")),
                plain("updatedBy", nn(T::STRING)),
            ],
        ),
        json_object(
            "TranslationKey",
            "A translatable message, e.g. `home.title`.",
            vec![
                plain("id", nn(T::ID)).doc("Derived from the namespace and name: it changes when the key is renamed."),
                plain("name", nn(T::STRING)),
                plain("namespace", nn(T::STRING)).doc("`\"\"` is the default namespace."),
                plain("description", T::named(T::STRING)).doc("Context for translators."),
                plain("tags", strs()),
                plain("plural", nn(T::BOOLEAN)),
                object("translations", T::named_nn_list_nn("Translation")),
                plain("createdAt", nn("DateTime")),
                plain("updatedAt", nn("DateTime")),
            ],
        ),
        json_object(
            "TranslationKeyConnection",
            "A page of keys.",
            vec![object("items", T::named_nn_list_nn("TranslationKey")), plain("total", nn(T::INT))],
        ),
        json_object(
            "TranslationLanguageStats",
            "Progress of one language.",
            vec![
                plain("language", nn(T::STRING)),
                plain("total", nn(T::INT)),
                plain("translated", nn(T::INT)).doc("Keys with a text (reviewed or not)."),
                plain("reviewed", nn(T::INT)),
            ],
        ),
        json_object(
            "TranslationFile",
            "An exported file.",
            vec![plain("path", nn(T::STRING)), plain("contentType", nn(T::STRING)), plain("content", nn(T::STRING))],
        ),
        json_object(
            "TranslationFormatInfo",
            "A supported file format.",
            vec![
                enumerated("format", nn("TranslationFormat")),
                plain("slug", nn(T::STRING)).doc("Name in URLs: `/i18n/{slug}/{file}`."),
                plain("contentType", nn(T::STRING)),
                plain("importable", nn(T::BOOLEAN)),
                plain("multiLanguage", nn(T::BOOLEAN)),
            ],
        ),
        json_object(
            "TranslationImportReport",
            "What an import changed.",
            vec![
                plain("keysCreated", nn(T::INT)),
                plain("keysUpdated", nn(T::INT)),
                plain("translationsCreated", nn(T::INT)),
                plain("translationsUpdated", nn(T::INT)),
                plain("translationsSkipped", nn(T::INT)),
                plain("languages", strs()),
            ],
        ),
        json_object(
            "TranslationSettings",
            "Live serving of translations at `/i18n/{format}/{file}`.",
            vec![
                plain("public", nn(T::BOOLEAN)).doc("Served without credentials."),
                enumerated("servedStates", T::named_nn_list_nn("TranslationState")),
                plain("fallbackToBase", nn(T::BOOLEAN)),
            ],
        ),
        json_object(
            "TranslationDeliveryTarget",
            "Object storage that exported translations are published to.",
            vec![
                plain("id", nn(T::ID)),
                plain("name", nn(T::STRING)),
                plain("enabled", nn(T::BOOLEAN)),
                enumerated("kind", nn("TranslationStorageKind")),
                plain("provider", nn(T::STRING)).doc("UI preset: garage, aws, r2, minio, azure, other."),
                plain("bucket", nn(T::STRING)).doc("S3 bucket or Azure container."),
                plain("endpoint", T::named(T::STRING)),
                plain("region", T::named(T::STRING)),
                plain("forcePathStyle", nn(T::BOOLEAN)),
                plain("accessKeyId", T::named(T::STRING)).doc("S3 access key id, or Azure account name."),
                plain("hasSecret", nn(T::BOOLEAN)).doc("Whether a secret key / SAS token is stored (never returned)."),
                enumerated("azureAuth", nn("TranslationAzureAuth")),
                plain("pathPrefix", nn(T::STRING)),
                plain("publicUrl", T::named(T::STRING)),
                enumerated("formats", T::named_nn_list_nn("TranslationFormat")),
                plain("languages", strs()),
                plain("namespaces", strs()),
                enumerated("states", T::named_nn_list_nn("TranslationState")),
                plain("fallbackToBase", nn(T::BOOLEAN)),
                plain("autoPublish", nn(T::BOOLEAN)),
                plain("lastPublishedAt", T::named("DateTime")),
                plain("lastError", T::named(T::STRING)),
                plain("lastFileCount", nn(T::INT)),
                plain("createdAt", nn("DateTime")),
                plain("updatedAt", nn("DateTime")),
            ],
        ),
        json_object(
            "TranslationPublishReport",
            "Outcome of publishing one target.",
            vec![
                plain("targetId", nn(T::ID)),
                plain("name", nn(T::STRING)),
                plain("ok", nn(T::BOOLEAN)),
                plain("files", nn(T::INT)),
                plain("error", T::named(T::STRING)),
                plain("publishedAt", nn("DateTime")),
            ],
        ),
    ];
    let enums = vec![
        enum_type("TranslationState", ["TRANSLATED", "REVIEWED"]),
        enum_type("TranslationStateFilter", ["UNTRANSLATED", "TRANSLATED", "REVIEWED"]),
        enum_type("TranslationFormat", Format::ALL.map(Format::as_str)),
        enum_type("TranslationImportMode", ["KEEP", "OVERRIDE"]),
        enum_type("TranslationStorageKind", ["S3", "AZURE_BLOB"]),
        enum_type("TranslationAzureAuth", ["SHARED_KEY", "SAS"]),
    ];
    let inputs = vec![
        InputObject::new("TranslationTextInput")
            .field(InputValue::new("language", nn(T::STRING)))
            .field(InputValue::new("text", nn(T::STRING))),
        InputObject::new("TranslationKeyInput")
            .field(InputValue::new("name", nn(T::STRING)))
            .field(InputValue::new("namespace", T::named(T::STRING)).default_value(""))
            .field(InputValue::new("description", T::named(T::STRING)))
            .field(InputValue::new("tags", T::named_nn_list(T::STRING)))
            .field(InputValue::new("plural", T::named(T::BOOLEAN)).default_value(false))
            .field(InputValue::new("translations", T::named_nn_list("TranslationTextInput"))),
        InputObject::new("TranslationKeyUpdateInput")
            .description("Omitted fields are left unchanged; `description: null` clears it.")
            .field(InputValue::new("name", T::named(T::STRING)))
            .field(InputValue::new("namespace", T::named(T::STRING)))
            .field(InputValue::new("description", T::named(T::STRING)))
            .field(InputValue::new("tags", T::named_nn_list(T::STRING)))
            .field(InputValue::new("plural", T::named(T::BOOLEAN))),
        InputObject::new("TranslationKeyFilter")
            .field(InputValue::new("search", T::named(T::STRING)).description("In names, descriptions and texts."))
            .field(InputValue::new("namespace", T::named(T::STRING)))
            .field(InputValue::new("tag", T::named(T::STRING)))
            .field(InputValue::new("language", T::named(T::STRING)))
            .field(
                InputValue::new("state", T::named("TranslationStateFilter"))
                    .description("In `language`, or in any language but the base one."),
            ),
        InputObject::new("TranslationImportInput")
            .field(InputValue::new("format", nn("TranslationFormat")))
            .field(InputValue::new("content", nn(T::STRING)).description("The file content (text)."))
            .field(InputValue::new("language", T::named(T::STRING)).description("For single-language files that do not declare it."))
            .field(InputValue::new("namespace", T::named(T::STRING)).default_value(""))
            .field(InputValue::new("mode", T::named("TranslationImportMode")).default_value(async_graphql::Value::Enum(async_graphql::Name::new("KEEP"))))
            .field(InputValue::new("state", T::named("TranslationState")).default_value(async_graphql::Value::Enum(async_graphql::Name::new("TRANSLATED"))))
            .field(InputValue::new("tags", T::named_nn_list(T::STRING)).description("Added to the keys the import creates."))
            .field(InputValue::new("createLanguages", T::named(T::BOOLEAN)).default_value(false).description("Admins only.")),
        InputObject::new("TranslationExportInput")
            .field(InputValue::new("format", nn("TranslationFormat")))
            .field(InputValue::new("languages", T::named_nn_list(T::STRING)))
            .field(InputValue::new("namespaces", T::named_nn_list(T::STRING)))
            .field(InputValue::new("states", T::named_nn_list("TranslationState")))
            .field(InputValue::new("fallbackToBase", T::named(T::BOOLEAN)).default_value(false)),
        InputObject::new("TranslationSettingsInput")
            .field(InputValue::new("public", T::named(T::BOOLEAN)))
            .field(InputValue::new("servedStates", T::named_nn_list("TranslationState")))
            .field(InputValue::new("fallbackToBase", T::named(T::BOOLEAN))),
        target_input("TranslationDeliveryTargetInput", true),
        target_input("TranslationDeliveryTargetUpdateInput", false),
    ];
    objects
        .into_iter()
        .map(TypeDef::Object)
        .chain(enums.into_iter().map(TypeDef::Enum))
        .chain(inputs.into_iter().map(TypeDef::Input))
        .collect()
}

fn target_input(name: &str, create: bool) -> InputObject {
    use TypeRef as T;
    let req = |t: &str| if create { T::named_nn(t) } else { T::named(t) };
    let mut i = InputObject::new(name)
        .field(InputValue::new("name", req(T::STRING)))
        .field(InputValue::new("enabled", T::named(T::BOOLEAN)))
        .field(InputValue::new("provider", T::named(T::STRING)))
        .field(InputValue::new("bucket", req(T::STRING)))
        .field(InputValue::new("endpoint", T::named(T::STRING)))
        .field(InputValue::new("region", T::named(T::STRING)))
        .field(InputValue::new("forcePathStyle", T::named(T::BOOLEAN)))
        .field(InputValue::new("accessKeyId", T::named(T::STRING)))
        .field(InputValue::new("secret", T::named(T::STRING)).description("Write-only. On update: omit to keep, null to remove."))
        .field(InputValue::new("azureAuth", T::named("TranslationAzureAuth")))
        .field(InputValue::new("pathPrefix", T::named(T::STRING)))
        .field(InputValue::new("publicUrl", T::named(T::STRING)))
        .field(InputValue::new("formats", if create { T::named_nn_list_nn("TranslationFormat") } else { T::named_nn_list("TranslationFormat") }))
        .field(InputValue::new("languages", T::named_nn_list(T::STRING)))
        .field(InputValue::new("namespaces", T::named_nn_list(T::STRING)))
        .field(InputValue::new("states", T::named_nn_list("TranslationState")))
        .field(InputValue::new("fallbackToBase", T::named(T::BOOLEAN)))
        .field(InputValue::new("autoPublish", T::named(T::BOOLEAN)));
    if create {
        i = i.field(InputValue::new("kind", T::named_nn("TranslationStorageKind")));
    }
    i
}

fn str_arg(ctx: &ResolverContext<'_>, name: &str) -> GqlResult<String> {
    Ok(ctx.args.try_get(name)?.string()?.to_string())
}

fn opt_str_arg(ctx: &ResolverContext<'_>, name: &str) -> GqlResult<Option<String>> {
    match ctx.args.get(name) {
        Some(v) if !v.is_null() => Ok(Some(v.string()?.to_string())),
        _ => Ok(None),
    }
}

fn opt_bool_arg(ctx: &ResolverContext<'_>, name: &str) -> GqlResult<Option<bool>> {
    match ctx.args.get(name) {
        Some(v) if !v.is_null() => Ok(Some(v.boolean()?)),
        _ => Ok(None),
    }
}

fn format_info(f: Format) -> Value {
    json!({
        "format": f.as_str(), "slug": f.slug(), "contentType": f.content_type(),
        "importable": f.importable(), "multiLanguage": f.multi_language(),
    })
}

fn queries() -> Vec<Field> {
    use TypeRef as T;
    let languages = Field::new("translationLanguages", T::named_nn_list_nn("TranslationLanguage"), |ctx| {
        FieldFuture::new(async move {
            require_write(&ctx)?;
            let all = service(&ctx)?.languages().await.map_err(cms_err)?;
            Ok(Some(values(all.iter().map(|l| json!(l)))))
        })
    })
    .description("Base language first.");
    let keys = Field::new("translationKeys", T::named_nn("TranslationKeyConnection"), |ctx| {
        FieldFuture::new(async move {
            require_write(&ctx)?;
            let f = json_arg(&ctx, "filter")?;
            let filter = KeyFilter {
                search: s(&f, "search"),
                namespace: s(&f, "namespace"),
                tag: s(&f, "tag"),
                language: s(&f, "language"),
                state: s(&f, "state"),
            };
            let first = ctx.args.get("first").map(|v| v.i64()).transpose()?.unwrap_or(50).clamp(0, 1000) as usize;
            let skip = ctx.args.get("skip").map(|v| v.i64()).transpose()?.unwrap_or(0).max(0) as usize;
            let (items, total) = service(&ctx)?.keys(&filter, skip, first).await.map_err(cms_err)?;
            Ok(Some(value(json!({ "items": items.iter().map(key_json).collect::<Vec<_>>(), "total": total }))))
        })
    })
    .description("Sorted by namespace, then name.")
    .argument(InputValue::new("filter", T::named("TranslationKeyFilter")))
    .argument(InputValue::new("first", T::named(T::INT)).default_value(50))
    .argument(InputValue::new("skip", T::named(T::INT)).default_value(0));
    let key = Field::new("translationKey", T::named("TranslationKey"), |ctx| {
        FieldFuture::new(async move {
            require_write(&ctx)?;
            match service(&ctx)?.key(&str_arg(&ctx, "id")?).await {
                Ok(k) => Ok(Some(value(key_json(&k)))),
                Err(CmsError::NotFound(_)) => Ok(None),
                Err(e) => Err(cms_err(e)),
            }
        })
    })
    .argument(InputValue::new("id", T::named_nn(T::ID)));
    let namespaces = Field::new("translationNamespaces", T::named_nn_list_nn(T::STRING), |ctx| {
        FieldFuture::new(async move {
            require_write(&ctx)?;
            let all = service(&ctx)?.namespaces().await.map_err(cms_err)?;
            Ok(Some(FieldValue::list(all.into_iter().map(FieldValue::value))))
        })
    });
    let tags = Field::new("translationTags", T::named_nn_list_nn(T::STRING), |ctx| {
        FieldFuture::new(async move {
            require_write(&ctx)?;
            let all = service(&ctx)?.tags().await.map_err(cms_err)?;
            Ok(Some(FieldValue::list(all.into_iter().map(FieldValue::value))))
        })
    });
    let stats = Field::new("translationStats", T::named_nn_list_nn("TranslationLanguageStats"), |ctx| {
        FieldFuture::new(async move {
            require_write(&ctx)?;
            let all = service(&ctx)?.stats().await.map_err(cms_err)?;
            Ok(Some(values(all.iter().map(|s| json!(s)))))
        })
    });
    let formats = Field::new("translationFormats", T::named_nn_list_nn("TranslationFormatInfo"), |ctx| {
        FieldFuture::new(async move {
            require_write(&ctx)?;
            Ok(Some(values(Format::ALL.into_iter().map(format_info))))
        })
    });
    let export = Field::new("exportTranslations", T::named_nn_list_nn("TranslationFile"), |ctx| {
        FieldFuture::new(async move {
            require_write(&ctx)?;
            let v = json_arg(&ctx, "input")?;
            let format = format_of(&v, "format")?.ok_or_else(|| bad("format is required"))?;
            let opts = ExportOptions {
                format,
                languages: list(&v, "languages").unwrap_or_default(),
                namespaces: list(&v, "namespaces").unwrap_or_default(),
                states: states_of(&v, "states").filter(|s| !s.is_empty()).unwrap_or_else(|| TranslationState::ALL.to_vec()),
                fallback_to_base: b(&v, "fallbackToBase").unwrap_or(false),
            };
            let files = service(&ctx)?.export(&opts).await.map_err(cms_err)?;
            Ok(Some(values(files.iter().map(|f| json!(f)))))
        })
    })
    .description("As text; GET /api/i18n/export downloads the same files (zipped when several).")
    .argument(InputValue::new("input", T::named_nn("TranslationExportInput")));
    let settings = Field::new("translationSettings", T::named_nn("TranslationSettings"), |ctx| {
        FieldFuture::new(async move {
            require_write(&ctx)?;
            Ok(Some(value(json!(service(&ctx)?.settings().await.map_err(cms_err)?))))
        })
    });
    let targets = Field::new("translationDeliveryTargets", T::named_nn_list_nn("TranslationDeliveryTarget"), |ctx| {
        FieldFuture::new(async move {
            admin_require(&ctx)?;
            let all = service(&ctx)?.targets().await.map_err(cms_err)?;
            Ok(Some(values(all.iter().map(target_json))))
        })
    });
    vec![languages, keys, key, namespaces, tags, stats, formats, export, settings, targets]
}

fn key_out(k: TranslationKey) -> Option<FieldValue<'static>> {
    Some(value(key_json(&k)))
}

fn mutations() -> Vec<Field> {
    use TypeRef as T;
    let add_language = Field::new("addTranslationLanguage", T::named_nn("TranslationLanguage"), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let (tag, name) = (str_arg(&ctx, "tag")?, opt_str_arg(&ctx, "name")?.unwrap_or_default());
            let base = opt_bool_arg(&ctx, "base")?.unwrap_or(false);
            let l = service(&ctx)?.add_language(&tag, &name, base, &actor).await.map_err(cms_err)?;
            Ok(Some(value(json!(l))))
        })
    })
    .argument(InputValue::new("tag", T::named_nn(T::STRING)))
    .argument(InputValue::new("name", T::named(T::STRING)))
    .argument(InputValue::new("base", T::named(T::BOOLEAN)));
    let update_language = Field::new("updateTranslationLanguage", T::named_nn("TranslationLanguage"), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let l = service(&ctx)?
                .update_language(&str_arg(&ctx, "tag")?, opt_str_arg(&ctx, "name")?, opt_bool_arg(&ctx, "base")?, &actor)
                .await
                .map_err(cms_err)?;
            Ok(Some(value(json!(l))))
        })
    })
    .argument(InputValue::new("tag", T::named_nn(T::STRING)))
    .argument(InputValue::new("name", T::named(T::STRING)))
    .argument(InputValue::new("base", T::named(T::BOOLEAN)));
    let remove_language = Field::new("removeTranslationLanguage", T::named_nn(T::STRING), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let tag = str_arg(&ctx, "tag")?;
            service(&ctx)?.remove_language(&tag, &actor).await.map_err(cms_err)?;
            Ok(Some(FieldValue::value(tag)))
        })
    })
    .description("Removes the language and ALL of its translations.")
    .argument(InputValue::new("tag", T::named_nn(T::STRING)));

    let create_key = Field::new("createTranslationKey", T::named_nn("TranslationKey"), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let v = json_arg(&ctx, "input")?;
            let translations = v
                .get("translations")
                .and_then(Value::as_array)
                .map(|xs| xs.iter().filter_map(|t| Some((s(t, "language")?, s(t, "text")?))).collect())
                .unwrap_or_default();
            let input = KeyInput {
                name: s(&v, "name").unwrap_or_default(),
                namespace: s(&v, "namespace").unwrap_or_default(),
                description: s(&v, "description"),
                tags: list(&v, "tags").unwrap_or_default(),
                plural: b(&v, "plural").unwrap_or(false),
                translations,
            };
            Ok(key_out(service(&ctx)?.create_key(input, &actor).await.map_err(cms_err)?))
        })
    })
    .argument(InputValue::new("input", T::named_nn("TranslationKeyInput")));
    let update_key = Field::new("updateTranslationKey", T::named_nn("TranslationKey"), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let v = json_arg(&ctx, "input")?;
            let update = KeyUpdate {
                name: s(&v, "name"),
                namespace: s(&v, "namespace"),
                description: nullable(&v, "description"),
                tags: list(&v, "tags"),
                plural: b(&v, "plural"),
            };
            Ok(key_out(service(&ctx)?.update_key(&str_arg(&ctx, "id")?, update, &actor).await.map_err(cms_err)?))
        })
    })
    .description("Renaming or moving a key gives it a new id.")
    .argument(InputValue::new("id", T::named_nn(T::ID)))
    .argument(InputValue::new("input", T::named_nn("TranslationKeyUpdateInput")));
    let delete_keys = Field::new("deleteTranslationKeys", T::named_nn(T::INT), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let ids: Vec<String> = ctx
                .args
                .try_get("ids")?
                .list()?
                .iter()
                .map(|v| v.string().map(String::from))
                .collect::<Result<_, _>>()?;
            let n = service(&ctx)?.delete_keys(&ids, &actor).await.map_err(cms_err)?;
            Ok(Some(FieldValue::value(n as i64)))
        })
    })
    .description("Returns how many keys were deleted.")
    .argument(InputValue::new("ids", T::named_nn_list_nn(T::ID)));
    let set = Field::new("setTranslation", T::named_nn("TranslationKey"), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let state = match ctx.args.get("state") {
                Some(v) if !v.is_null() => TranslationState::parse(v.enum_name()?),
                _ => None,
            };
            let k = service(&ctx)?
                .set_translation(&str_arg(&ctx, "keyId")?, &str_arg(&ctx, "language")?, opt_str_arg(&ctx, "text")?, state, &actor)
                .await
                .map_err(cms_err)?;
            Ok(key_out(k))
        })
    })
    .description(
        "Sets a text (`\"\"` removes it) and/or its state. Changing a text sends it back to TRANSLATED unless `state` says otherwise; REVIEWED needs the review permission.",
    )
    .argument(InputValue::new("keyId", T::named_nn(T::ID)))
    .argument(InputValue::new("language", T::named_nn(T::STRING)))
    .argument(InputValue::new("text", T::named(T::STRING)))
    .argument(InputValue::new("state", T::named("TranslationState")));
    let import = Field::new("importTranslations", T::named_nn("TranslationImportReport"), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let v = json_arg(&ctx, "input")?;
            let input = ImportInput {
                format: format_of(&v, "format")?.ok_or_else(|| bad("format is required"))?,
                content: s(&v, "content").unwrap_or_default(),
                language: s(&v, "language").filter(|l| !l.is_empty()),
                namespace: s(&v, "namespace").unwrap_or_default(),
                mode: if s(&v, "mode").as_deref() == Some("OVERRIDE") { ImportMode::Override } else { ImportMode::Keep },
                state: s(&v, "state").and_then(|x| TranslationState::parse(&x)).unwrap_or(TranslationState::Translated),
                tags: list(&v, "tags").unwrap_or_default(),
                create_languages: b(&v, "createLanguages").unwrap_or(false),
            };
            let report = service(&ctx)?.import(input, &actor).await.map_err(cms_err)?;
            Ok(Some(value(json!(report))))
        })
    })
    .argument(InputValue::new("input", T::named_nn("TranslationImportInput")));
    let settings = Field::new("updateTranslationSettings", T::named_nn("TranslationSettings"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let v = json_arg(&ctx, "input")?;
            let svc = service(&ctx)?;
            let mut current: I18nSettings = svc.settings().await.map_err(cms_err)?;
            if let Some(p) = b(&v, "public") {
                current.public = p;
            }
            if let Some(states) = states_of(&v, "servedStates") {
                if states.is_empty() {
                    return Err(bad("serve at least one state"));
                }
                current.served_states = states;
            }
            if let Some(f) = b(&v, "fallbackToBase") {
                current.fallback_to_base = f;
            }
            Ok(Some(value(json!(svc.update_settings(current, &actor).await.map_err(cms_err)?))))
        })
    })
    .argument(InputValue::new("input", T::named_nn("TranslationSettingsInput")));

    let create_target = Field::new("createTranslationDeliveryTarget", T::named_nn("TranslationDeliveryTarget"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let v = json_arg(&ctx, "input")?;
            let kind = kind_of(&v).ok_or_else(|| bad("kind is required"))?;
            let input = DeliveryInput {
                name: s(&v, "name").unwrap_or_default(),
                enabled: b(&v, "enabled").unwrap_or(true),
                kind,
                provider: s(&v, "provider").unwrap_or_else(|| if kind == StorageKind::AzureBlob { "azure" } else { "other" }.into()),
                bucket: s(&v, "bucket").unwrap_or_default(),
                endpoint: s(&v, "endpoint"),
                region: s(&v, "region"),
                force_path_style: b(&v, "forcePathStyle").unwrap_or(false),
                access_key_id: s(&v, "accessKeyId"),
                secret: s(&v, "secret"),
                azure_auth: azure_auth_of(&v).unwrap_or_default(),
                path_prefix: s(&v, "pathPrefix").unwrap_or_default(),
                public_url: s(&v, "publicUrl"),
                formats: formats_of(&v, "formats")?.unwrap_or_default(),
                languages: list(&v, "languages").unwrap_or_default(),
                namespaces: list(&v, "namespaces").unwrap_or_default(),
                states: states_of(&v, "states").filter(|s| !s.is_empty()).unwrap_or_else(|| TranslationState::ALL.to_vec()),
                fallback_to_base: b(&v, "fallbackToBase").unwrap_or(false),
                auto_publish: b(&v, "autoPublish").unwrap_or(false),
            };
            let t = service(&ctx)?.create_target(input, &actor).await.map_err(cms_err)?;
            Ok(Some(value(target_json(&t))))
        })
    })
    .argument(InputValue::new("input", T::named_nn("TranslationDeliveryTargetInput")));
    let update_target = Field::new("updateTranslationDeliveryTarget", T::named_nn("TranslationDeliveryTarget"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let v = json_arg(&ctx, "input")?;
            let update = DeliveryUpdate {
                name: s(&v, "name"),
                enabled: b(&v, "enabled"),
                provider: s(&v, "provider"),
                bucket: s(&v, "bucket"),
                endpoint: nullable(&v, "endpoint"),
                region: nullable(&v, "region"),
                force_path_style: b(&v, "forcePathStyle"),
                access_key_id: nullable(&v, "accessKeyId"),
                secret: nullable(&v, "secret"),
                azure_auth: azure_auth_of(&v),
                path_prefix: s(&v, "pathPrefix"),
                public_url: nullable(&v, "publicUrl"),
                formats: formats_of(&v, "formats")?,
                languages: list(&v, "languages"),
                namespaces: list(&v, "namespaces"),
                states: states_of(&v, "states"),
                fallback_to_base: b(&v, "fallbackToBase"),
                auto_publish: b(&v, "autoPublish"),
            };
            let t = service(&ctx)?.update_target(&str_arg(&ctx, "id")?, update, &actor).await.map_err(cms_err)?;
            Ok(Some(value(target_json(&t))))
        })
    })
    .argument(InputValue::new("id", T::named_nn(T::ID)))
    .argument(InputValue::new("input", T::named_nn("TranslationDeliveryTargetUpdateInput")));
    let delete_target = Field::new("deleteTranslationDeliveryTarget", T::named_nn(T::ID), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let id = str_arg(&ctx, "id")?;
            service(&ctx)?.delete_target(&id, &actor).await.map_err(cms_err)?;
            Ok(Some(FieldValue::value(id)))
        })
    })
    .argument(InputValue::new("id", T::named_nn(T::ID)));
    let publish = Field::new("publishTranslations", T::named_nn_list_nn("TranslationPublishReport"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let id = opt_str_arg(&ctx, "targetId")?;
            let reports = service(&ctx)?.publish(id.as_deref(), &actor).await.map_err(cms_err)?;
            Ok(Some(values(reports.iter().map(|r| json!(r)))))
        })
    })
    .description("Publishes one target, or every enabled target when `targetId` is omitted.")
    .argument(InputValue::new("targetId", T::named(T::ID)));

    vec![
        add_language,
        update_language,
        remove_language,
        create_key,
        update_key,
        delete_keys,
        set,
        import,
        settings,
        create_target,
        update_target,
        delete_target,
        publish,
    ]
}

pub(crate) fn definitions() -> (Vec<Field>, Vec<Field>, Vec<TypeDef>) {
    (queries(), mutations(), types())
}
