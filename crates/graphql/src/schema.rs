// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Builds the dynamic GraphQL schema from the current content types.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_graphql::dynamic::{
    Enum, Field, FieldFuture, FieldValue, InputObject, InputValue, Object, ResolverContext, Scalar,
    Schema, SchemaError, TypeRef, Union,
};
use async_graphql::{Error as GqlError, ErrorExtensions, Name, Value as GqlValue};
use serde_json::{Map, Value};
use tess_core::query::EntryQuery;
use tess_core::validate::Components;
use tess_core::{Actor, Cms, CmsError, ContentType, ContentTypeDef, Entry, FieldType, Stage};
use uuid::Uuid;

use crate::fields::{decode_input, enum_type_name, input_type, output_field, scalar_type_name};
use crate::filter::{
    FilterKind, decode_where_ids, filterable_fields, parse_order_by, parse_where, sortable_fields,
};
use crate::ids::{IdCodec, IdError};
use crate::naming::Names;
use crate::{Principal, SchemaConfig};

/// Set by schema-changing mutations so the next request rebuilds the schema.
#[derive(Clone)]
pub(crate) struct SchemaDirty(pub Arc<AtomicBool>);

/// Parent value of every content object.
pub(crate) struct EntryNode {
    pub(crate) entry: Entry,
    pub(crate) stage: Stage,
    pub(crate) view: Map<String, Value>,
}

impl EntryNode {
    pub(crate) fn value(entry: Entry, stage: Stage) -> Option<FieldValue<'static>> {
        let view = entry.view(stage)?;
        Some(FieldValue::owned_any(EntryNode { entry, stage, view }))
    }
}

pub(crate) struct Connection {
    pub(crate) items: Vec<EntryNode>,
    pub(crate) total: u64,
}

pub(crate) type GqlResult<T> = Result<T, GqlError>;

pub(crate) fn cms_err(err: CmsError) -> GqlError {
    let code = match &err {
        CmsError::NotFound(_) => "NOT_FOUND",
        CmsError::Validation(_) => "BAD_USER_INPUT",
        CmsError::Conflict(_) => "CONFLICT",
        CmsError::Forbidden(_) => "FORBIDDEN",
        CmsError::Storage(_) => "INTERNAL_SERVER_ERROR",
        CmsError::Plugin(_) => "PLUGIN_ERROR",
    };
    let message = match &err {
        CmsError::Storage(detail) => {
            tracing::error!(error = %detail, "storage failure");
            "internal storage error".to_string()
        }
        other => other.to_string(),
    };
    GqlError::new(message).extend_with(|_, e| e.set("code", code))
}

fn forbidden() -> GqlError {
    GqlError::new("not authorized").extend_with(|_, e| e.set("code", "FORBIDDEN"))
}

pub(crate) fn principal(ctx: &ResolverContext<'_>) -> Principal {
    ctx.data_opt::<Principal>()
        .cloned()
        .unwrap_or(Principal::Anonymous)
}

pub(crate) fn require_read(ctx: &ResolverContext<'_>, stage: Stage) -> GqlResult<()> {
    if principal(ctx).can_read(stage) {
        Ok(())
    } else {
        Err(forbidden())
    }
}

/// The authenticated actor; fine-grained permissions are checked by `Cms`.
pub(crate) fn require_write(ctx: &ResolverContext<'_>) -> GqlResult<Actor> {
    match principal(ctx) {
        Principal::User(actor) => Ok(actor),
        _ => Err(GqlError::new("sign in required").extend_with(|_, e| e.set("code", "FORBIDDEN"))),
    }
}

/// An optional argument; an explicit `null` counts as absent.
pub(crate) fn opt_arg<'a>(
    ctx: &'a ResolverContext<'_>,
    name: &str,
) -> Option<async_graphql::dynamic::ValueAccessor<'a>> {
    ctx.args.get(name).filter(|v| !v.is_null())
}

/// An optional integer argument with a default (also used for explicit `null`).
pub(crate) fn int_arg(ctx: &ResolverContext<'_>, name: &str, default: i64) -> GqlResult<i64> {
    opt_arg(ctx, name)
        .map(|v| v.i64())
        .transpose()
        .map(|v| v.unwrap_or(default))
}

pub(crate) fn stage_arg(ctx: &ResolverContext<'_>) -> GqlResult<Stage> {
    match opt_arg(ctx, "stage") {
        Some(v) if v.enum_name()? == "DRAFT" => Ok(Stage::Draft),
        _ => Ok(Stage::Published),
    }
}

pub(crate) fn json_arg(ctx: &ResolverContext<'_>, name: &str) -> GqlResult<Value> {
    match opt_arg(ctx, name) {
        Some(v) => Ok(v.as_value().clone().into_json()?),
        None => Ok(Value::Null),
    }
}

fn object_arg(ctx: &ResolverContext<'_>, name: &str) -> GqlResult<Map<String, Value>> {
    match json_arg(ctx, name)? {
        Value::Object(m) => Ok(m),
        _ => Err(GqlError::new(format!("`{name}` must be an object"))),
    }
}

pub(crate) fn codec<'a>(ctx: &ResolverContext<'a>) -> GqlResult<&'a IdCodec> {
    Ok(ctx.data::<Arc<IdCodec>>()?.as_ref())
}

pub(crate) fn id_err(err: IdError) -> GqlError {
    let code = if err == IdError::Expired {
        "ID_EXPIRED"
    } else {
        "BAD_USER_INPUT"
    };
    GqlError::new(err.to_string()).extend_with(|_, e| e.set("code", code))
}

/// Decodes the opaque `id` argument, which must refer to a `type_name`.
pub(crate) fn id_arg(ctx: &ResolverContext<'_>, type_name: &str) -> GqlResult<Uuid> {
    let token = ctx.args.try_get("id")?.string()?;
    codec(ctx)?
        .decode(token, Some(type_name))
        .map(|(_, id)| id)
        .map_err(id_err)
}

fn stage_value(stage: Stage) -> GqlValue {
    GqlValue::Enum(Name::new(match stage {
        Stage::Draft => "DRAFT",
        Stage::Published => "PUBLISHED",
    }))
}

pub(crate) fn stage_input() -> InputValue {
    InputValue::new("stage", TypeRef::named("Stage")).default_value(stage_value(Stage::Published))
}

/// What the schema is generated from.
pub(crate) struct Model<'a> {
    pub content_types: &'a [ContentType],
    pub components: &'a Components,
    /// Content types with search enabled (they get `search<Plural>`).
    pub searchable: &'a std::collections::BTreeSet<String>,
}

pub(crate) fn build(
    model: Model<'_>,
    config: &SchemaConfig,
    cms: Cms,
    codec: Arc<IdCodec>,
    dirty: SchemaDirty,
    services: &crate::Services,
) -> Result<Schema, SchemaError> {
    let Model {
        content_types,
        components,
        searchable,
    } = model;
    let mut query = Object::new("Query");
    let mut mutation = Object::new("Mutation");
    let mut builder = Schema::build("Query", Some("Mutation"), None);

    builder = builder
        .register(Enum::new("Stage").item("DRAFT").item("PUBLISHED"))
        .register(crate::workflow::status_enum())
        .register(crate::workflow::history_action_enum())
        .register(crate::workflow::history_event_type())
        .register(
            Scalar::new("DateTime")
                .description("RFC 3339 date-time, e.g. 2026-01-31T12:00:00.000Z"),
        )
        .register(Scalar::new("Date").description("Calendar date without time, e.g. 2026-01-31"))
        .register(
            Scalar::new("UUID").description("UUID, e.g. 6f9619ff-8b86-d011-b42d-00cf4fc964ff"),
        )
        .register(Scalar::new("JSON").description("Arbitrary JSON value"));
    for ty in crate::components::component_types(components) {
        builder = ty.register(builder);
    }
    query = query.field(verify_password_query());

    for ct in content_types {
        let ct = Arc::new(ct.clone());
        let names = Names::of(&ct);
        let extra = crate::media::extra_object_fields(&ct, services);
        for ty in content_type_types(&ct, &names, extra)
            .into_iter()
            .chain(crate::components::zone_types(&ct))
        {
            builder = ty.register(builder);
        }
        query = query
            .field(single_query(&ct, &names))
            .field(list_query(&ct, &names, config));
        if services.search.is_some() && searchable.contains(&ct.api_id) {
            query = query.field(crate::search::search_query(&ct, &names, config));
        }
        for field in entry_mutations(&ct, &names) {
            mutation = mutation.field(field);
        }
        if ct.workflow {
            for field in crate::workflow::mutations(&ct, &names) {
                mutation = mutation.field(field);
            }
        }
    }

    let (admin_queries, admin_mutations, admin_types) = crate::admin::definitions();
    for f in admin_queries {
        query = query.field(f);
    }
    for f in admin_mutations {
        mutation = mutation.field(f);
    }
    for ty in admin_types {
        builder = ty.register(builder);
    }
    let (q, m, types) = crate::components::definitions();
    query = q.into_iter().fold(query, |o, f| o.field(f));
    mutation = m.into_iter().fold(mutation, |o, f| o.field(f));
    for ty in types {
        builder = ty.register(builder);
    }
    let (media_mutations, media_types) = crate::media::definitions(content_types, services);
    for f in media_mutations {
        mutation = mutation.field(f);
    }
    for ty in media_types {
        builder = ty.register(builder);
    }
    if let Some(media) = &services.media {
        builder = builder.data(media.clone());
    }
    if let Some(auth) = &services.auth {
        let (q, m, types) = crate::accounts::definitions();
        query = q.into_iter().fold(query, |o, f| o.field(f));
        mutation = m.into_iter().fold(mutation, |o, f| o.field(f));
        for ty in types {
            builder = ty.register(builder);
        }
        builder = builder.data(auth.clone());
    }
    let (q, m, types) = crate::search::definitions(services.search.is_some());
    query = q.into_iter().fold(query, |o, f| o.field(f));
    mutation = m.into_iter().fold(mutation, |o, f| o.field(f));
    for ty in types {
        builder = ty.register(builder);
    }
    if let Some(search) = &services.search {
        builder = builder.data(search.clone());
    }
    if let Some(plugins) = &services.plugins {
        let (q, m, types) = crate::plugins::definitions();
        query = q.into_iter().fold(query, |o, f| o.field(f));
        mutation = m.into_iter().fold(mutation, |o, f| o.field(f));
        for ty in types {
            builder = ty.register(builder);
        }
        builder = builder.data(plugins.clone());
    }

    if let Some(i18n) = &services.i18n {
        let (q, m, types) = crate::i18n::definitions();
        query = q.into_iter().fold(query, |o, f| o.field(f));
        mutation = m.into_iter().fold(mutation, |o, f| o.field(f));
        for ty in types {
            builder = ty.register(builder);
        }
        builder = builder.data(i18n.clone());
    }
    if let Some(forms) = &services.forms {
        let (q, m, types) = crate::forms::definitions();
        query = q.into_iter().fold(query, |o, f| o.field(f));
        mutation = m.into_iter().fold(mutation, |o, f| o.field(f));
        for ty in types {
            builder = ty.register(builder);
        }
        builder = builder.data(forms.clone());
    }
    if let Some(webhooks) = &services.webhooks {
        let (q, m, types) = crate::webhooks::definitions();
        query = q.into_iter().fold(query, |o, f| o.field(f));
        mutation = m.into_iter().fold(mutation, |o, f| o.field(f));
        for ty in types {
            builder = ty.register(builder);
        }
        builder = builder.data(webhooks.clone());
    }
    builder = builder.register(crate::jsonobj::json_object(
        "WorkflowEditor",
        "The embedded workflow editor.",
        vec![
            crate::jsonobj::plain("provider", TypeRef::named_nn(TypeRef::STRING)),
            crate::jsonobj::plain("url", TypeRef::named_nn(TypeRef::STRING))
                .doc("Same-origin URL of the editor."),
        ],
    ));
    // Never with the commercial license, even if an editor is configured.
    let workflows = services
        .workflows
        .clone()
        .filter(|_| services.license != crate::License::Commercial);
    query = query.field(
        Field::new(
            "workflowEditor",
            TypeRef::named("WorkflowEditor"),
            move |ctx| {
                let workflows = workflows.clone();
                FieldFuture::new(async move {
                    require_write(&ctx)?;
                    Ok(workflows.map(|w| {
                        crate::jsonobj::value(
                            serde_json::json!({ "provider": w.provider, "url": w.editor_url }),
                        )
                    }))
                })
            },
        )
        .description("Null when no workflow editor is configured."),
    );
    builder = builder.register(Enum::new("License").item("COMMUNITY").item("COMMERCIAL"));
    let license = services.license;
    query = query.field(
        Field::new("license", TypeRef::named_nn("License"), move |ctx| {
            FieldFuture::new(async move {
                require_write(&ctx)?;
                Ok(Some(FieldValue::value(GqlValue::Enum(Name::new(
                    license.as_str(),
                )))))
            })
        })
        .description("The license this server runs under. Signed-in users only."),
    );

    let mut builder = builder
        .register(query)
        .register(mutation)
        .data(cms)
        .data(Arc::new(components.clone()))
        .data(codec)
        .data(dirty)
        .limit_depth(config.max_depth);
    if !config.introspection {
        builder = builder.disable_introspection();
    }
    builder.finish()
}

/// A type to register: lets the per-content-type builder return a mixed list.
pub(crate) enum TypeDef {
    Object(Object),
    Input(InputObject),
    Enum(Enum),
    Union(Union),
}

impl TypeDef {
    pub(crate) fn register(
        self,
        b: async_graphql::dynamic::SchemaBuilder,
    ) -> async_graphql::dynamic::SchemaBuilder {
        match self {
            TypeDef::Object(o) => b.register(o),
            TypeDef::Input(i) => b.register(i),
            TypeDef::Enum(e) => b.register(e),
            TypeDef::Union(u) => b.register(u),
        }
    }
}

fn content_type_types(ct: &Arc<ContentType>, n: &Names, extra_fields: Vec<Field>) -> Vec<TypeDef> {
    let mut types = Vec::new();

    // Enums of enum fields.
    for f in &ct.fields {
        if let FieldType::Enum { values } = &f.field_type {
            types.push(TypeDef::Enum(
                Enum::new(enum_type_name(&ct.api_id, &f.name))
                    .items(values.iter().map(String::as_str)),
            ));
        }
    }

    // Object type.
    let mut obj = Object::new(&n.object);
    if let Some(d) = &ct.description {
        obj = obj.description(d);
    }
    obj = obj
        .field(
            Field::new("id", TypeRef::named_nn(TypeRef::ID), |ctx| {
                FieldFuture::new(async move {
                    let node = ctx.parent_value.try_downcast_ref::<EntryNode>()?;
                    let id = codec(&ctx)?.encode(&node.entry.content_type, node.entry.id);
                    Ok(Some(FieldValue::value(id)))
                })
            })
            .description("Opaque id; expires, refetch the object to get a fresh one."),
        )
        .field(
            Field::new("ref", TypeRef::named_nn(TypeRef::STRING), |ctx| {
                FieldFuture::new(async move {
                    let node = ctx.parent_value.try_downcast_ref::<EntryNode>()?;
                    let r = codec(&ctx)?.stable_ref(&node.entry.content_type, node.entry.id);
                    Ok(Some(FieldValue::value(r)))
                })
            })
            .description("Stable opaque reference, for client-side caching and comparison."),
        )
        .field(view_field("createdAt", TypeRef::named_nn("DateTime")))
        .field(view_field("updatedAt", TypeRef::named_nn("DateTime")))
        .field(view_field("publishedAt", TypeRef::named("DateTime")))
        .field(Field::new("stage", TypeRef::named_nn("Stage"), |ctx| {
            FieldFuture::new(async move {
                let node = ctx.parent_value.try_downcast_ref::<EntryNode>()?;
                Ok(Some(FieldValue::value(stage_value(node.stage))))
            })
        }));
    for f in crate::workflow::entry_fields() {
        obj = obj.field(f);
    }
    for f in &ct.fields {
        if let Some(field) = output_field(&ct.api_id, f) {
            obj = obj.field(field);
        }
    }
    for f in extra_fields {
        obj = obj.field(f);
    }
    types.push(TypeDef::Object(obj));

    // Connection.
    types.push(TypeDef::Object(
        Object::new(&n.connection)
            .field(Field::new(
                "items",
                TypeRef::named_nn_list_nn(&n.object),
                |ctx| {
                    FieldFuture::new(async move {
                        let conn = ctx.parent_value.try_downcast_ref::<Connection>()?;
                        Ok(Some(FieldValue::list(
                            conn.items.iter().map(|n| FieldValue::borrowed_any(n)),
                        )))
                    })
                },
            ))
            .field(Field::new(
                "total",
                TypeRef::named_nn(TypeRef::INT),
                |ctx| {
                    FieldFuture::new(async move {
                        let conn = ctx.parent_value.try_downcast_ref::<Connection>()?;
                        Ok(Some(FieldValue::value(conn.total as i64)))
                    })
                },
            )),
    ));

    // Where input.
    let mut where_input = InputObject::new(&n.where_input)
        .field(InputValue::new(
            "AND",
            TypeRef::named_nn_list(&n.where_input),
        ))
        .field(InputValue::new(
            "OR",
            TypeRef::named_nn_list(&n.where_input),
        ))
        .field(InputValue::new(
            "NOT",
            TypeRef::named_nn_list(&n.where_input),
        ));
    for (name, kind) in filterable_fields(ct) {
        let scalar = match ct.field(&name) {
            Some(f) => scalar_type_name(&ct.api_id, f),
            None if kind == FilterKind::Id => TypeRef::ID.to_string(),
            None if name == "workflowStatus" => "WorkflowStatus".to_string(),
            None => "DateTime".to_string(),
        };
        where_input = where_input.field(InputValue::new(&name, TypeRef::named(&scalar)));
        for suffix in kind.suffixes() {
            let ty = if suffix.ends_with("in") {
                TypeRef::named_nn_list(&scalar)
            } else {
                TypeRef::named(&scalar)
            };
            where_input = where_input.field(InputValue::new(format!("{name}_{suffix}"), ty));
        }
    }
    types.push(TypeDef::Input(where_input));

    // Order by.
    let mut order = Enum::new(&n.order_by);
    for f in sortable_fields(ct) {
        order = order.item(format!("{f}_ASC")).item(format!("{f}_DESC"));
    }
    types.push(TypeDef::Enum(order));

    // Create / update inputs.
    let mut create = InputObject::new(&n.create_input);
    let mut update = InputObject::new(&n.update_input);
    for f in &ct.fields {
        create = create.field(InputValue::new(
            &f.name,
            input_type(&ct.api_id, f, f.required),
        ));
        update = update.field(InputValue::new(&f.name, input_type(&ct.api_id, f, false)));
    }
    types.push(TypeDef::Input(create));
    types.push(TypeDef::Input(update));
    types
}

/// Field read straight from the entry view (system fields).
fn view_field(name: &'static str, ty: TypeRef) -> Field {
    Field::new(name, ty, move |ctx| {
        FieldFuture::new(async move {
            let node = ctx.parent_value.try_downcast_ref::<EntryNode>()?;
            Ok(node
                .view
                .get(name)
                .and_then(|v| GqlValue::from_json(v.clone()).ok())
                .map(FieldValue::value))
        })
    })
}

fn single_query(ct: &Arc<ContentType>, n: &Names) -> Field {
    let api_id = ct.api_id.clone();
    Field::new(&n.single_query, TypeRef::named(&n.object), move |ctx| {
        let api_id = api_id.clone();
        FieldFuture::new(async move {
            let stage = stage_arg(&ctx)?;
            require_read(&ctx, stage)?;
            let id = id_arg(&ctx, &api_id)?;
            let entry = ctx
                .data::<Cms>()?
                .get_entry(&api_id, id, stage)
                .await
                .map_err(cms_err)?;
            Ok(entry.and_then(|e| EntryNode::value(e, stage)))
        })
    })
    .description(format!("A single {} by id.", ct.api_id))
    .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::ID)))
    .argument(stage_input())
}

fn list_query(ct: &Arc<ContentType>, n: &Names, config: &SchemaConfig) -> Field {
    let ct_ref = ct.clone();
    let max = config.max_page_size;
    let default_first = config.default_page_size as i64;
    Field::new(
        &n.list_query,
        TypeRef::named_nn(&n.connection),
        move |ctx| {
            let ct = ct_ref.clone();
            FieldFuture::new(async move {
                let stage = stage_arg(&ctx)?;
                require_read(&ctx, stage)?;
                let first = int_arg(&ctx, "first", default_first)?;
                let skip = int_arg(&ctx, "skip", 0)?;
                if first < 0 || skip < 0 {
                    return Err(cms_err(CmsError::validation(
                        "`first` and `skip` must be positive",
                    )));
                }
                let codec = codec(&ctx)?;
                let mut where_input = json_arg(&ctx, "where")?;
                decode_where_ids(&ct, &mut where_input, &|token, ty| {
                    codec
                        .decode(token, Some(ty))
                        .map(|(_, id)| id.to_string())
                        .map_err(|e| CmsError::validation(e.to_string()))
                })
                .map_err(cms_err)?;
                let query = EntryQuery {
                    stage,
                    filter: parse_where(&ct, &where_input).map_err(cms_err)?,
                    sort: parse_order_by(&json_arg(&ctx, "orderBy")?).map_err(cms_err)?,
                    limit: (first as usize).min(max),
                    offset: skip as usize,
                };
                let page = ctx
                    .data::<Cms>()?
                    .query_entries(&ct.api_id, &query)
                    .await
                    .map_err(cms_err)?;
                let items = page
                    .items
                    .into_iter()
                    .filter_map(|entry| {
                        let view = entry.view(stage)?;
                        Some(EntryNode { entry, stage, view })
                    })
                    .collect();
                Ok(Some(FieldValue::owned_any(Connection {
                    items,
                    total: page.total,
                })))
            })
        },
    )
    .description(format!(
        "List {} entries, at most {max} per page.",
        ct.api_id
    ))
    .argument(InputValue::new("where", TypeRef::named(&n.where_input)))
    .argument(InputValue::new(
        "orderBy",
        TypeRef::named_nn_list(&n.order_by),
    ))
    .argument(
        InputValue::new("first", TypeRef::named(TypeRef::INT))
            .default_value(config.default_page_size as i64),
    )
    .argument(InputValue::new("skip", TypeRef::named(TypeRef::INT)).default_value(0))
    .argument(stage_input())
}

fn entry_mutations(ct: &Arc<ContentType>, n: &Names) -> Vec<Field> {
    let api = ct.api_id.clone();
    let ct_ref = ct.clone();
    let id_input = || InputValue::new("id", TypeRef::named_nn(TypeRef::ID));

    let ct = ct_ref.clone();
    let create = Field::new(&n.create, TypeRef::named(&n.object), move |ctx| {
        let ct = ct.clone();
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let mut data = object_arg(&ctx, "data")?;
            decode_input(
                &ct.fields,
                &mut data,
                codec(&ctx)?,
                ctx.data::<Arc<Components>>()?,
            )?;
            let entry = ctx
                .data::<Cms>()?
                .create_entry(&ct.api_id, data, &actor)
                .await
                .map_err(cms_err)?;
            Ok(EntryNode::value(entry, Stage::Draft))
        })
    })
    .description(format!("Create a {} draft.", api))
    .argument(InputValue::new("data", TypeRef::named_nn(&n.create_input)));

    let ct = ct_ref;
    let update = Field::new(&n.update, TypeRef::named(&n.object), move |ctx| {
        let ct = ct.clone();
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let id = id_arg(&ctx, &ct.api_id)?;
            let mut data = object_arg(&ctx, "data")?;
            decode_input(
                &ct.fields,
                &mut data,
                codec(&ctx)?,
                ctx.data::<Arc<Components>>()?,
            )?;
            let entry = ctx
                .data::<Cms>()?
                .update_entry(&ct.api_id, id, data, &actor)
                .await
                .map_err(cms_err)?;
            Ok(EntryNode::value(entry, Stage::Draft))
        })
    })
    .description("Update the draft; `null` clears a field.")
    .argument(id_input())
    .argument(InputValue::new("data", TypeRef::named_nn(&n.update_input)));

    let a = api.clone();
    let delete = Field::new(&n.delete, TypeRef::named(TypeRef::ID), move |ctx| {
        let a = a.clone();
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let id = id_arg(&ctx, &a)?;
            ctx.data::<Cms>()?
                .delete_entry(&a, id, &actor)
                .await
                .map_err(cms_err)?;
            Ok(Some(FieldValue::value(codec(&ctx)?.encode(&a, id))))
        })
    })
    .argument(id_input());

    let a = api.clone();
    let publish = Field::new(&n.publish, TypeRef::named(&n.object), move |ctx| {
        let a = a.clone();
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let id = id_arg(&ctx, &a)?;
            let entry = ctx
                .data::<Cms>()?
                .publish_entry(&a, id, &actor)
                .await
                .map_err(cms_err)?;
            Ok(EntryNode::value(entry, Stage::Published))
        })
    })
    .description("Copy the current draft to the PUBLISHED stage.")
    .argument(id_input());

    let a = api;
    let unpublish = Field::new(&n.unpublish, TypeRef::named(&n.object), move |ctx| {
        let a = a.clone();
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let id = id_arg(&ctx, &a)?;
            let entry = ctx
                .data::<Cms>()?
                .unpublish_entry(&a, id, &actor)
                .await
                .map_err(cms_err)?;
            Ok(EntryNode::value(entry, Stage::Draft))
        })
    })
    .argument(id_input());

    vec![create, update, delete, publish, unpublish]
}

/// `verifyPassword`: checks a candidate against a PASSWORD field.
fn verify_password_query() -> Field {
    Field::new(
        "verifyPassword",
        TypeRef::named_nn(TypeRef::BOOLEAN),
        |ctx| {
            FieldFuture::new(async move {
                let stage = stage_arg(&ctx)?;
                require_read(&ctx, stage)?;
                let content_type = ctx.args.try_get("contentType")?.string()?.to_string();
                let id = id_arg(&ctx, &content_type)?;
                let field = ctx.args.try_get("field")?.string()?;
                let password = ctx.args.try_get("password")?.string()?;
                let ok = ctx
                    .data::<Cms>()?
                    .verify_password(&content_type, id, field, stage, password)
                    .await
                    .map_err(cms_err)?;
                Ok(Some(FieldValue::value(ok)))
            })
        },
    )
    .description(
        "Check a password against a PASSWORD field of an entry (passwords are never returned).",
    )
    .argument(InputValue::new(
        "contentType",
        TypeRef::named_nn(TypeRef::STRING),
    ))
    .argument(InputValue::new("id", TypeRef::named_nn(TypeRef::ID)))
    .argument(InputValue::new("field", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new(
        "password",
        TypeRef::named_nn(TypeRef::STRING),
    ))
    .argument(stage_input())
}

// ---- helpers shared with the admin module ----------------------------------

pub(crate) fn admin_cms_err(err: CmsError) -> GqlError {
    cms_err(err)
}

/// An actor with the Admin (or Owner) role.
pub(crate) fn admin_require(ctx: &ResolverContext<'_>) -> GqlResult<Actor> {
    let actor = require_write(ctx)?;
    actor.require_admin().map_err(cms_err)?;
    Ok(actor)
}

pub(crate) fn admin_mark_dirty(ctx: &ResolverContext<'_>) -> GqlResult<()> {
    ctx.data::<SchemaDirty>()?.0.store(true, Ordering::SeqCst);
    Ok(())
}

pub(crate) fn admin_json_arg(ctx: &ResolverContext<'_>, name: &str) -> GqlResult<Value> {
    json_arg(ctx, name)
}

pub(crate) fn admin_parse_def(input: Value) -> Result<ContentTypeDef, CmsError> {
    crate::admin::parse_content_type_input(input)
}
