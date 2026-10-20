// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Forms management API (admin only). Visitors use the REST endpoints
//! (`GET`/`POST /api/forms/<slug>`).

use std::sync::Arc;

use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputObject, InputValue, ResolverContext, TypeRef,
};
use serde_json::{Value, json};
use tess_core::CmsError;
use tess_forms::{FieldKind, FormDef, FormField, FormInput, FormsService};
use uuid::Uuid;

use crate::jsonobj::{enum_type, enumerated, json_object, object, plain, value, values};
use crate::schema::{GqlResult, TypeDef, admin_require, cms_err, json_arg};

fn service<'a>(ctx: &ResolverContext<'a>) -> GqlResult<&'a Arc<FormsService>> {
    ctx.data::<Arc<FormsService>>()
}

fn uuid_arg(ctx: &ResolverContext<'_>, name: &str) -> GqlResult<Uuid> {
    let s = ctx.args.try_get(name)?.string()?;
    Uuid::parse_str(s)
        .map_err(|_| cms_err(CmsError::validation(format!("`{name}` is not a valid id"))))
}

fn form_json(f: &FormDef, submissions: usize) -> Value {
    let mut v = serde_json::to_value(f).unwrap_or_default();
    v["submissionCount"] = json!(submissions);
    v
}

fn input(v: &Value) -> GqlResult<FormInput> {
    let fields: Vec<FormField> =
        serde_json::from_value(v.get("fields").cloned().unwrap_or(Value::Null))
            .map_err(|e| cms_err(CmsError::validation(format!("fields: {e}"))))?;
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(String::from);
    Ok(FormInput {
        slug: s("slug").unwrap_or_default(),
        title: s("title").unwrap_or_default(),
        description: s("description"),
        fields,
        enabled: v.get("enabled").and_then(Value::as_bool).unwrap_or(true),
        success_message: s("successMessage"),
        redirect_url: s("redirectUrl"),
    })
}

fn types() -> Vec<TypeDef> {
    use TypeRef as T;
    let nn = T::named_nn;
    let kinds = enum_type("FormFieldType", FieldKind::ALL.iter().map(|k| k.as_str()));
    let field = json_object(
        "FormField",
        "A field of a form.",
        vec![
            plain("name", nn(T::STRING)).doc("Key of the value in submissions."),
            plain("label", nn(T::STRING)),
            enumerated("type", nn("FormFieldType")),
            plain("required", nn(T::BOOLEAN)),
            plain("help", T::named(T::STRING)),
            plain("placeholder", T::named(T::STRING)),
            plain("options", T::named_nn_list(T::STRING)).doc("SELECT and MULTI_SELECT choices."),
            plain("maxLength", T::named(T::INT)),
            plain("min", T::named(T::FLOAT)),
            plain("max", T::named(T::FLOAT)),
        ],
    );
    let form = json_object(
        "Form",
        "A custom form. Visitors read it at `GET /api/forms/<slug>` and submit to `POST /api/forms/<slug>`.",
        vec![
            plain("id", nn(T::ID)),
            plain("slug", nn(T::STRING)),
            plain("title", nn(T::STRING)),
            plain("description", T::named(T::STRING)),
            object("fields", T::named_nn_list_nn("FormField")),
            plain("enabled", nn(T::BOOLEAN)),
            plain("successMessage", T::named(T::STRING)),
            plain("redirectUrl", T::named(T::STRING))
                .doc("Plain HTML form posts are redirected here."),
            plain("submissionCount", nn(T::INT)),
            plain("createdBy", nn(T::STRING)),
            plain("createdAt", nn("DateTime")),
            plain("updatedAt", nn("DateTime")),
        ],
    );
    let submission = json_object(
        "FormSubmission",
        "A submitted form.",
        vec![
            plain("id", nn(T::ID)),
            plain("formId", nn(T::ID)),
            plain("data", nn("JSON")).doc("Values by field name."),
            plain("submittedAt", nn("DateTime")),
            plain("source", T::named(T::STRING)).doc("Page the form was posted from, when known."),
        ],
    );
    let page = json_object(
        "FormSubmissionConnection",
        "Submissions, newest first.",
        vec![
            plain("total", nn(T::INT)),
            object("items", T::named_nn_list_nn("FormSubmission")),
        ],
    );
    let field_input = InputObject::new("FormFieldInput")
        .field(InputValue::new("name", nn(T::STRING)))
        .field(InputValue::new("label", nn(T::STRING)))
        .field(InputValue::new("type", nn("FormFieldType")))
        .field(InputValue::new("required", T::named(T::BOOLEAN)).default_value(false))
        .field(InputValue::new("help", T::named(T::STRING)))
        .field(InputValue::new("placeholder", T::named(T::STRING)))
        .field(InputValue::new("options", T::named_nn_list(T::STRING)))
        .field(InputValue::new("maxLength", T::named(T::INT)))
        .field(InputValue::new("min", T::named(T::FLOAT)))
        .field(InputValue::new("max", T::named(T::FLOAT)));
    let form_input = InputObject::new("FormInput")
        .field(InputValue::new("slug", nn(T::STRING)))
        .field(InputValue::new("title", nn(T::STRING)))
        .field(InputValue::new("description", T::named(T::STRING)))
        .field(InputValue::new(
            "fields",
            T::named_nn_list_nn("FormFieldInput"),
        ))
        .field(InputValue::new("enabled", T::named(T::BOOLEAN)).default_value(true))
        .field(InputValue::new("successMessage", T::named(T::STRING)))
        .field(InputValue::new("redirectUrl", T::named(T::STRING)));
    vec![
        TypeDef::Enum(kinds),
        TypeDef::Object(field),
        TypeDef::Object(form),
        TypeDef::Object(submission),
        TypeDef::Object(page),
        TypeDef::Input(field_input),
        TypeDef::Input(form_input),
    ]
}

pub(crate) fn definitions() -> (Vec<Field>, Vec<Field>, Vec<TypeDef>) {
    use TypeRef as T;
    let forms = Field::new("forms", T::named_nn_list_nn("Form"), |ctx| {
        FieldFuture::new(async move {
            admin_require(&ctx)?;
            let svc = service(&ctx)?;
            let mut out = vec![];
            for f in svc.list().await.map_err(cms_err)? {
                let count = svc.submission_count(f.id).await.map_err(cms_err)?;
                out.push(form_json(&f, count));
            }
            Ok(Some(values(out)))
        })
    })
    .description("Admins.");
    let form = Field::new("form", T::named("Form"), |ctx| {
        FieldFuture::new(async move {
            admin_require(&ctx)?;
            let svc = service(&ctx)?;
            match svc.get(uuid_arg(&ctx, "id")?).await {
                Ok(f) => Ok(Some(value(form_json(
                    &f,
                    svc.submission_count(f.id).await.map_err(cms_err)?,
                )))),
                Err(CmsError::NotFound(_)) => Ok(None),
                Err(e) => Err(cms_err(e)),
            }
        })
    })
    .argument(InputValue::new("id", T::named_nn(T::ID)));
    let submissions = Field::new(
        "formSubmissions",
        T::named_nn("FormSubmissionConnection"),
        |ctx| {
            FieldFuture::new(async move {
                let actor = admin_require(&ctx)?;
                let first = ctx
                    .args
                    .get("first")
                    .map(|v| v.i64())
                    .transpose()?
                    .unwrap_or(50)
                    .clamp(1, 500) as usize;
                let skip = ctx
                    .args
                    .get("skip")
                    .map(|v| v.i64())
                    .transpose()?
                    .unwrap_or(0)
                    .max(0) as usize;
                let (items, total) = service(&ctx)?
                    .submissions(uuid_arg(&ctx, "formId")?, skip, first, &actor)
                    .await
                    .map_err(cms_err)?;
                let items: Vec<Value> = items
                    .iter()
                    .map(|s| serde_json::to_value(s).unwrap_or_default())
                    .collect();
                Ok(Some(value(json!({ "total": total, "items": items }))))
            })
        },
    )
    .description("Admins. Newest first.")
    .argument(InputValue::new("formId", T::named_nn(T::ID)))
    .argument(InputValue::new("first", T::named(T::INT)).default_value(50))
    .argument(InputValue::new("skip", T::named(T::INT)).default_value(0));

    let create = Field::new("createForm", T::named_nn("Form"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let f = service(&ctx)?
                .create(input(&json_arg(&ctx, "input")?)?, &actor)
                .await
                .map_err(cms_err)?;
            Ok(Some(value(form_json(&f, 0))))
        })
    })
    .argument(InputValue::new("input", T::named_nn("FormInput")));
    let update = Field::new("updateForm", T::named_nn("Form"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let svc = service(&ctx)?;
            let f = svc
                .update(
                    uuid_arg(&ctx, "id")?,
                    input(&json_arg(&ctx, "input")?)?,
                    &actor,
                )
                .await
                .map_err(cms_err)?;
            Ok(Some(value(form_json(
                &f,
                svc.submission_count(f.id).await.map_err(cms_err)?,
            ))))
        })
    })
    .description("Replaces the definition (submissions are kept).")
    .argument(InputValue::new("id", T::named_nn(T::ID)))
    .argument(InputValue::new("input", T::named_nn("FormInput")));
    let delete = Field::new("deleteForm", T::named_nn(T::ID), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let id = uuid_arg(&ctx, "id")?;
            service(&ctx)?.delete(id, &actor).await.map_err(cms_err)?;
            Ok(Some(FieldValue::value(id.to_string())))
        })
    })
    .description("Deletes the form and all its submissions.")
    .argument(InputValue::new("id", T::named_nn(T::ID)));
    let delete_submissions = Field::new("deleteFormSubmissions", T::named_nn(T::INT), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let ids: Vec<Uuid> = ctx
                .args
                .try_get("ids")?
                .list()?
                .iter()
                .filter_map(|v| v.string().ok().and_then(|s| Uuid::parse_str(s).ok()))
                .collect();
            let n = service(&ctx)?
                .delete_submissions(uuid_arg(&ctx, "formId")?, &ids, &actor)
                .await
                .map_err(cms_err)?;
            Ok(Some(FieldValue::value(n as i64)))
        })
    })
    .description("Returns how many were deleted.")
    .argument(InputValue::new("formId", T::named_nn(T::ID)))
    .argument(InputValue::new("ids", T::named_nn_list_nn(T::ID)));

    (
        vec![forms, form, submissions],
        vec![create, update, delete, delete_submissions],
        types(),
    )
}
