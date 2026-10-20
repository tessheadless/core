// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Components in the API: their object/input types, the union and input of
//! dynamic zones, and the management API (`components`, `createComponent`…).

use async_graphql::Value as GqlValue;
use async_graphql::dynamic::{
    Enum, Field, FieldFuture, FieldValue, InputObject, InputValue, Object, TypeRef, Union,
};
use serde_json::Value;
use tess_core::model::format_ts;
use tess_core::validate::Components;
use tess_core::{Cms, CmsError, Component, ComponentDef, ContentType, FieldType};

use crate::fields::{component_input_name, enum_type_name, input_type, output_field, zone_input_name, zone_type_name};
use crate::schema::{require_write, TypeDef, admin_cms_err, admin_json_arg, admin_mark_dirty, admin_require};

/// Object, input and enum types of every component.
pub(crate) fn component_types(components: &Components) -> Vec<TypeDef> {
    let mut types = vec![];
    for c in components.values() {
        let mut obj = Object::new(&c.api_id);
        if let Some(d) = &c.description {
            obj = obj.description(d);
        }
        let mut input = InputObject::new(component_input_name(&c.api_id));
        for f in &c.fields {
            if let FieldType::Enum { values } = &f.field_type {
                types.push(TypeDef::Enum(
                    Enum::new(enum_type_name(&c.api_id, &f.name)).items(values.iter().map(String::as_str)),
                ));
            }
            if let Some(field) = output_field(&c.api_id, f) {
                obj = obj.field(field);
            }
            input = input.field(InputValue::new(&f.name, input_type(&c.api_id, f, f.required)));
        }
        types.push(TypeDef::Object(obj));
        types.push(TypeDef::Input(input));
    }
    types
}

/// Union (output) and one-of input of each dynamic zone of a content type.
pub(crate) fn zone_types(ct: &ContentType) -> Vec<TypeDef> {
    let mut types = vec![];
    for f in &ct.fields {
        let FieldType::DynamicZone { components } = &f.field_type else { continue };
        let mut union = Union::new(zone_type_name(&ct.api_id, &f.name));
        let mut input = InputObject::new(zone_input_name(&ct.api_id, &f.name))
            .description("Set exactly one field: the component of this item.");
        for c in components {
            union = union.possible_type(c);
            input = input.field(InputValue::new(c, TypeRef::named(component_input_name(c))));
        }
        types.push(TypeDef::Union(union));
        types.push(TypeDef::Input(input));
    }
    types
}

// ---- management API -------------------------------------------------------

fn component_field(name: &'static str, ty: TypeRef, get: fn(&Component) -> Option<GqlValue>) -> Field {
    Field::new(name, ty, move |ctx| {
        FieldFuture::new(async move {
            let c = ctx.parent_value.try_downcast_ref::<Component>()?;
            Ok(get(c).map(FieldValue::value))
        })
    })
}

fn parse_component_input(input: Value) -> Result<ComponentDef, CmsError> {
    let obj = input.as_object().ok_or_else(|| CmsError::validation("input must be an object"))?;
    Ok(ComponentDef {
        api_id: obj.get("apiId").and_then(Value::as_str).unwrap_or_default().to_string(),
        description: obj.get("description").and_then(Value::as_str).map(String::from),
        fields: crate::admin::parse_fields(obj.get("fields").cloned().unwrap_or(Value::Null))?,
    })
}

pub(crate) fn definitions() -> (Vec<Field>, Vec<Field>, Vec<TypeDef>) {
    let object = Object::new("ComponentDefinition")
        .description("A reusable group of fields, used by COMPONENT and DYNAMIC_ZONE fields.")
        .field(component_field("apiId", TypeRef::named_nn(TypeRef::STRING), |c| Some(c.api_id.clone().into())))
        .field(component_field("description", TypeRef::named(TypeRef::STRING), |c| c.description.clone().map(Into::into)))
        .field(component_field("createdAt", TypeRef::named_nn("DateTime"), |c| Some(format_ts(c.created_at).into())))
        .field(component_field("updatedAt", TypeRef::named_nn("DateTime"), |c| Some(format_ts(c.updated_at).into())))
        .field(Field::new("fields", TypeRef::named_nn_list_nn("ContentTypeField"), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<Component>()?;
                Ok(Some(FieldValue::list(c.fields.iter().map(|f| FieldValue::borrowed_any(f)))))
            })
        }));
    let input = InputObject::new("ComponentDefinitionInput")
        .field(InputValue::new("apiId", TypeRef::named_nn(TypeRef::STRING)))
        .field(InputValue::new("description", TypeRef::named(TypeRef::STRING)))
        .field(InputValue::new("fields", TypeRef::named_nn_list_nn("ContentTypeFieldInput")));

    let list = Field::new("components", TypeRef::named_nn_list_nn("ComponentDefinition"), |ctx| {
        FieldFuture::new(async move {
            // Readable by every signed-in user (forms in the content manager).
            require_write(&ctx)?;
            let all = ctx.data::<Cms>()?.components().await.map_err(admin_cms_err)?;
            Ok(Some(FieldValue::list(all.into_values().map(FieldValue::owned_any))))
        })
    });
    let get = Field::new("component", TypeRef::named("ComponentDefinition"), |ctx| {
        FieldFuture::new(async move {
            // Readable by every signed-in user (forms in the content manager).
            require_write(&ctx)?;
            let api_id = ctx.args.try_get("apiId")?.string()?;
            let mut all = ctx.data::<Cms>()?.components().await.map_err(admin_cms_err)?;
            Ok(all.remove(api_id).map(FieldValue::owned_any))
        })
    })
    .argument(InputValue::new("apiId", TypeRef::named_nn(TypeRef::STRING)));

    let create = Field::new("createComponent", TypeRef::named_nn("ComponentDefinition"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let def = parse_component_input(admin_json_arg(&ctx, "input")?).map_err(admin_cms_err)?;
            let c = ctx.data::<Cms>()?.create_component(def, &actor).await.map_err(admin_cms_err)?;
            admin_mark_dirty(&ctx)?;
            Ok(Some(FieldValue::owned_any(c)))
        })
    })
    .argument(InputValue::new("input", TypeRef::named_nn("ComponentDefinitionInput")));

    let update = Field::new("updateComponent", TypeRef::named_nn("ComponentDefinition"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let api_id = ctx.args.try_get("apiId")?.string()?.to_string();
            let def = parse_component_input(admin_json_arg(&ctx, "input")?).map_err(admin_cms_err)?;
            let c = ctx.data::<Cms>()?.update_component(&api_id, def, &actor).await.map_err(admin_cms_err)?;
            admin_mark_dirty(&ctx)?;
            Ok(Some(FieldValue::owned_any(c)))
        })
    })
    .description("Replace a component's definition (renaming is not supported). Every content type using it changes.")
    .argument(InputValue::new("apiId", TypeRef::named_nn(TypeRef::STRING)))
    .argument(InputValue::new("input", TypeRef::named_nn("ComponentDefinitionInput")));

    let delete = Field::new("deleteComponent", TypeRef::named_nn(TypeRef::STRING), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let api_id = ctx.args.try_get("apiId")?.string()?.to_string();
            ctx.data::<Cms>()?.delete_component(&api_id, &actor).await.map_err(admin_cms_err)?;
            admin_mark_dirty(&ctx)?;
            Ok(Some(FieldValue::value(api_id)))
        })
    })
    .description("Delete a component that no content type or component uses anymore.")
    .argument(InputValue::new("apiId", TypeRef::named_nn(TypeRef::STRING)));

    (vec![list, get], vec![create, update, delete], vec![TypeDef::Object(object), TypeDef::Input(input)])
}
