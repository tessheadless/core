// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! GraphQL surface of the media library: `Asset.url` and direct uploads.

use async_graphql::dynamic::{Field, FieldFuture, FieldValue, InputValue, Object, TypeRef};
use serde_json::Value;
use tess_core::{ContentType, Stage};
use tess_media::{ASSET_TYPE, MediaService};

use crate::Services;
use crate::schema::{EntryNode, GqlResult, TypeDef, cms_err, codec, id_err, require_write};

/// Extra fields on generated object types.
pub(crate) fn extra_object_fields(ct: &ContentType, services: &Services) -> Vec<Field> {
    if ct.api_id != ASSET_TYPE || services.media.is_none() {
        return vec![];
    }
    vec![
        Field::new("url", TypeRef::named(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let node = ctx.parent_value.try_downcast_ref::<EntryNode>()?;
                let Some(key) = node.view.get("key").and_then(Value::as_str) else {
                    return Ok(None);
                };
                let url = ctx
                    .data::<MediaService>()?
                    .url(key)
                    .await
                    .map_err(cms_err)?;
                Ok(Some(FieldValue::value(url)))
            })
        })
        .description("Download URL (CDN, presigned, or proxied by the CMS)."),
    ]
}

struct Upload {
    asset: EntryNode,
    request: tess_media::PresignedRequest,
}

pub(crate) fn definitions(
    content_types: &[ContentType],
    services: &Services,
) -> (Vec<Field>, Vec<TypeDef>) {
    if services.media.is_none() || !content_types.iter().any(|c| c.api_id == ASSET_TYPE) {
        return (vec![], vec![]);
    }
    let header = Object::new("HttpHeader")
        .field(Field::new(
            "name",
            TypeRef::named_nn(TypeRef::STRING),
            |ctx| {
                FieldFuture::new(async move {
                    let (k, _) = ctx.parent_value.try_downcast_ref::<(String, String)>()?;
                    Ok(Some(FieldValue::value(k.clone())))
                })
            },
        ))
        .field(Field::new(
            "value",
            TypeRef::named_nn(TypeRef::STRING),
            |ctx| {
                FieldFuture::new(async move {
                    let (_, v) = ctx.parent_value.try_downcast_ref::<(String, String)>()?;
                    Ok(Some(FieldValue::value(v.clone())))
                })
            },
        ));
    let upload = Object::new("AssetUpload")
        .description("Send the file with `method` to `url`, with exactly these `headers`, then call confirmAssetUpload.")
        .field(Field::new("asset", TypeRef::named_nn(ASSET_TYPE), |ctx| {
            FieldFuture::new(async move {
                let u = ctx.parent_value.try_downcast_ref::<Upload>()?;
                Ok(Some(FieldValue::borrowed_any(&u.asset)))
            })
        }))
        .field(Field::new("url", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let u = ctx.parent_value.try_downcast_ref::<Upload>()?;
                Ok(Some(FieldValue::value(u.request.url.clone())))
            })
        }))
        .field(Field::new("method", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                let u = ctx.parent_value.try_downcast_ref::<Upload>()?;
                Ok(Some(FieldValue::value(u.request.method.clone())))
            })
        }))
        .field(Field::new("headers", TypeRef::named_nn_list_nn("HttpHeader"), |ctx| {
            FieldFuture::new(async move {
                let u = ctx.parent_value.try_downcast_ref::<Upload>()?;
                Ok(Some(FieldValue::list(u.request.headers.iter().map(|h| FieldValue::borrowed_any(h)))))
            })
        }))
        .field(Field::new("expiresIn", TypeRef::named_nn(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                let u = ctx.parent_value.try_downcast_ref::<Upload>()?;
                Ok(Some(FieldValue::value(u.request.expires_in.as_secs() as i64)))
            })
        }));

    let create = Field::new(
        "createAssetUpload",
        TypeRef::named_nn("AssetUpload"),
        |ctx| {
            FieldFuture::new(async move {
                let actor = require_write(&ctx)?;
                let file_name = ctx.args.try_get("fileName")?.string()?;
                let mime_type = ctx.args.try_get("mimeType")?.string()?;
                let size = ctx.args.try_get("size")?.i64()?.max(0) as u64;
                let cms = ctx.data::<tess_core::Cms>()?;
                let (entry, request) = ctx
                    .data::<MediaService>()?
                    .create_upload(cms, &actor, file_name, mime_type, size)
                    .await
                    .map_err(cms_err)?;
                let asset = node(entry)?;
                Ok(Some(FieldValue::owned_any(Upload { asset, request })))
            })
        },
    )
    .description("Start a direct upload to the object store (S3-compatible stores only).")
    .argument(InputValue::new(
        "fileName",
        TypeRef::named_nn(TypeRef::STRING),
    ))
    .argument(InputValue::new(
        "mimeType",
        TypeRef::named_nn(TypeRef::STRING),
    ))
    .argument(InputValue::new("size", TypeRef::named_nn(TypeRef::INT)));

    let confirm = Field::new("confirmAssetUpload", TypeRef::named_nn(ASSET_TYPE), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let token = ctx.args.try_get("id")?.string()?;
            let (_, id) = codec(&ctx)?
                .decode(token, Some(ASSET_TYPE))
                .map_err(id_err)?;
            let cms = ctx.data::<tess_core::Cms>()?;
            let entry = ctx
                .data::<MediaService>()?
                .confirm_upload(cms, &actor, id)
                .await
                .map_err(cms_err)?;
            Ok(Some(FieldValue::owned_any(node(entry)?)))
        })
    })
    .description("Mark a direct upload as finished, after checking the object exists.")
    .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::ID)));

    (
        vec![create, confirm],
        vec![TypeDef::Object(header), TypeDef::Object(upload)],
    )
}

fn node(entry: tess_core::Entry) -> GqlResult<EntryNode> {
    let view = entry
        .view(Stage::Draft)
        .ok_or_else(|| async_graphql::Error::new("asset has no draft"))?;
    Ok(EntryNode {
        entry,
        stage: Stage::Draft,
        view,
    })
}
