// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Plugin management API (admin only).

use std::sync::Arc;

use async_graphql::Value as GqlValue;
use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputObject, InputValue, Object, ResolverContext, TypeRef,
};
use base64::Engine;
use serde_json::Value;
use tess_core::{Cms, CmsError};
use tess_plugins::{PluginLimits, PluginManager, PluginSettings, PluginStatus};

use crate::schema::{GqlResult, TypeDef, admin_require, cms_err, json_arg};

fn manager<'a>(ctx: &ResolverContext<'a>) -> GqlResult<&'a PluginManager> {
    Ok(ctx.data::<Arc<PluginManager>>()?.as_ref())
}

fn status_field(
    name: &'static str,
    ty: TypeRef,
    get: fn(&PluginStatus) -> Option<GqlValue>,
) -> Field {
    Field::new(name, ty, move |ctx| {
        FieldFuture::new(async move {
            let s = ctx.parent_value.try_downcast_ref::<PluginStatus>()?;
            Ok(get(s).map(FieldValue::value))
        })
    })
}

fn limits_field(name: &'static str, ty: TypeRef, get: fn(&PluginLimits) -> GqlValue) -> Field {
    Field::new(name, ty, move |ctx| {
        FieldFuture::new(async move {
            let l = ctx.parent_value.try_downcast_ref::<PluginLimits>()?;
            Ok(Some(FieldValue::value(get(l))))
        })
    })
}

fn strings(xs: &[String]) -> GqlValue {
    GqlValue::List(xs.iter().cloned().map(GqlValue::from).collect())
}

fn types() -> Vec<TypeDef> {
    use TypeRef as T;
    let nn = T::named_nn;
    let plugin = Object::new("Plugin")
        .description("An installed WebAssembly plugin.")
        .field(status_field("name", nn(T::STRING), |s| {
            Some(s.record.name.clone().into())
        }))
        .field(status_field("version", T::named(T::STRING), |s| {
            s.record.version.clone().map(Into::into)
        }))
        .field(status_field("description", T::named(T::STRING), |s| {
            s.record.description.clone().map(Into::into)
        }))
        .field(status_field("enabled", nn(T::BOOLEAN), |s| {
            Some(s.record.enabled.into())
        }))
        .field(
            status_field("loaded", nn(T::BOOLEAN), |s| Some(s.loaded.into())).description(
                "Whether the plugin is currently running on the instance serving this request.",
            ),
        )
        .field(status_field("lastError", T::named(T::STRING), |s| {
            s.last_error.clone().map(Into::into)
        }))
        .field(status_field(
            "exports",
            T::named_nn_list_nn(T::STRING),
            |s| Some(strings(&s.record.exports)),
        ))
        .field(
            status_field("contentTypes", T::named_nn_list_nn(T::STRING), |s| {
                Some(strings(&s.record.content_types))
            })
            .description("Content types the plugin applies to; empty means all."),
        )
        .field(status_field(
            "allowedHosts",
            T::named_nn_list_nn(T::STRING),
            |s| Some(strings(&s.record.allowed_hosts)),
        ))
        .field(status_field("config", nn("JSON"), |s| {
            GqlValue::from_json(s.record.config.clone()).ok()
        }))
        .field(status_field("timeoutMs", nn(T::INT), |s| {
            Some((s.record.timeout_ms as i64).into())
        }))
        .field(status_field("memoryMaxPages", nn(T::INT), |s| {
            Some((s.record.memory_max_pages as i64).into())
        }))
        .field(status_field("failOpen", nn(T::BOOLEAN), |s| {
            Some(s.record.fail_open.into())
        }))
        .field(status_field("sha256", nn(T::STRING), |s| {
            Some(s.record.sha256.clone().into())
        }))
        .field(status_field("size", nn(T::INT), |s| {
            Some((s.record.size as i64).into())
        }))
        .field(status_field("installedAt", nn("DateTime"), |s| {
            Some(s.record.installed_at.clone().into())
        }))
        .field(status_field("updatedAt", nn("DateTime"), |s| {
            Some(s.record.updated_at.clone().into())
        }));

    let limits = Object::new("PluginLimits")
        .field(limits_field("maxModuleBytes", nn(T::INT), |l| {
            (l.max_module_bytes as i64).into()
        }))
        .field(limits_field("maxTimeoutMs", nn(T::INT), |l| {
            (l.max_timeout_ms as i64).into()
        }))
        .field(limits_field("maxMemoryPages", nn(T::INT), |l| {
            (l.max_memory_pages as i64).into()
        }))
        .field(limits_field("allowHttp", nn(T::BOOLEAN), |l| {
            l.allow_http.into()
        }));

    let input = InputObject::new("PluginSettingsInput")
        .description("Omitted fields are left unchanged.")
        .field(InputValue::new("enabled", T::named(T::BOOLEAN)))
        .field(InputValue::new("config", T::named("JSON")))
        .field(InputValue::new("contentTypes", T::named_nn_list(T::STRING)))
        .field(InputValue::new("allowedHosts", T::named_nn_list(T::STRING)))
        .field(InputValue::new("timeoutMs", T::named(T::INT)))
        .field(InputValue::new("memoryMaxPages", T::named(T::INT)))
        .field(InputValue::new("failOpen", T::named(T::BOOLEAN)));

    vec![
        TypeDef::Object(plugin),
        TypeDef::Object(limits),
        TypeDef::Input(input),
    ]
}

fn parse_settings(v: Value) -> Result<PluginSettings, CmsError> {
    let o = v.as_object().cloned().unwrap_or_default();
    let list = |k: &str| -> Option<Vec<String>> {
        o.get(k)?.as_array().map(|xs| {
            xs.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
    };
    let int = |k: &str| o.get(k).and_then(Value::as_i64);
    if [int("timeoutMs"), int("memoryMaxPages")]
        .iter()
        .flatten()
        .any(|v| *v <= 0)
    {
        return Err(CmsError::validation(
            "timeoutMs and memoryMaxPages must be positive",
        ));
    }
    Ok(PluginSettings {
        enabled: o.get("enabled").and_then(Value::as_bool),
        config: o.get("config").filter(|v| !v.is_null()).cloned(),
        content_types: list("contentTypes"),
        allowed_hosts: list("allowedHosts"),
        timeout_ms: int("timeoutMs").map(|v| v as u64),
        memory_max_pages: int("memoryMaxPages").map(|v| v.min(u32::MAX as i64) as u32),
        fail_open: o.get("failOpen").and_then(Value::as_bool),
    })
}

/// Applies plugin changes on this instance right away (others pick them up
/// on their next refresh).
async fn reload(ctx: &ResolverContext<'_>) -> GqlResult<()> {
    manager(ctx)?
        .refresh(ctx.data::<Cms>()?)
        .await
        .map_err(cms_err)
}

pub(crate) fn definitions() -> (Vec<Field>, Vec<Field>, Vec<TypeDef>) {
    let list = Field::new("plugins", TypeRef::named_nn_list_nn("Plugin"), |ctx| {
        FieldFuture::new(async move {
            admin_require(&ctx)?;
            let all = manager(&ctx)?.list().await.map_err(cms_err)?;
            Ok(Some(FieldValue::list(
                all.into_iter().map(FieldValue::owned_any),
            )))
        })
    });
    let get = Field::new("plugin", TypeRef::named("Plugin"), |ctx| {
        FieldFuture::new(async move {
            admin_require(&ctx)?;
            let name = ctx.args.try_get("name")?.string()?;
            match manager(&ctx)?.get(name).await {
                Ok(s) => Ok(Some(FieldValue::owned_any(s))),
                Err(CmsError::NotFound(_)) => Ok(None),
                Err(e) => Err(cms_err(e)),
            }
        })
    })
    .argument(InputValue::new("name", TypeRef::named_nn(TypeRef::STRING)));
    let limits = Field::new("pluginLimits", TypeRef::named_nn("PluginLimits"), |ctx| {
        FieldFuture::new(async move {
            admin_require(&ctx)?;
            Ok(Some(FieldValue::owned_any(manager(&ctx)?.limits().clone())))
        })
    });

    let install = Field::new("installPlugin", TypeRef::named_nn("Plugin"), |ctx| {
        FieldFuture::new(async move {
            admin_require(&ctx)?;
            let name = ctx.args.try_get("name")?.string()?.to_string();
            let wasm = base64::engine::general_purpose::STANDARD
                .decode(ctx.args.try_get("wasmBase64")?.string()?.trim())
                .map_err(|_| cms_err(CmsError::validation("wasmBase64 is not valid base64")))?;
            let m = manager(&ctx)?;
            m.install(&name, wasm.into()).await.map_err(cms_err)?;
            reload(&ctx).await?;
            Ok(Some(FieldValue::owned_any(m.get(&name).await.map_err(cms_err)?)))
        })
    })
    .description("Install or replace a plugin module (installed disabled). Large modules: POST the file to /api/plugins/{name}.")
    .argument(InputValue::new("name", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("wasmBase64", TypeRef::named_nn(TypeRef::STRING)));

    let update = Field::new("updatePlugin", TypeRef::named_nn("Plugin"), |ctx| {
        FieldFuture::new(async move {
            admin_require(&ctx)?;
            let name = ctx.args.try_get("name")?.string()?.to_string();
            let settings = parse_settings(json_arg(&ctx, "input")?).map_err(cms_err)?;
            let m = manager(&ctx)?;
            m.update(&name, settings).await.map_err(cms_err)?;
            reload(&ctx).await?;
            Ok(Some(FieldValue::owned_any(
                m.get(&name).await.map_err(cms_err)?,
            )))
        })
    })
    .argument(InputValue::new("name", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new(
        "input",
        TypeRef::named_nn("PluginSettingsInput"),
    ));

    let uninstall = Field::new(
        "uninstallPlugin",
        TypeRef::named_nn(TypeRef::STRING),
        |ctx| {
            FieldFuture::new(async move {
                admin_require(&ctx)?;
                let name = ctx.args.try_get("name")?.string()?.to_string();
                manager(&ctx)?.uninstall(&name).await.map_err(cms_err)?;
                reload(&ctx).await?;
                Ok(Some(FieldValue::value(name)))
            })
        },
    )
    .argument(InputValue::new("name", TypeRef::named_nn(TypeRef::STRING)));

    (
        vec![list, get, limits],
        vec![install, update, uninstall],
        types(),
    )
}
