// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Users, sessions and API tokens in the API.

use std::sync::Arc;
use std::time::Duration;

use async_graphql::dynamic::{
    Enum, Field, FieldFuture, FieldValue, InputObject, InputValue, Object, ResolverContext, TypeRef,
};
use async_graphql::{Name, Value as GqlValue};
use serde_json::Value;
use tess_auth::{ApiToken, AuthService, NewUser, Session, User, UserUpdate};
use tess_core::model::format_ts;
use tess_core::{CmsError, Role};

use crate::Principal;
use crate::schema::{
    GqlResult, TypeDef, admin_require, cms_err, codec, id_err, json_arg, opt_arg, principal,
    require_write,
};

const USER_TYPE: &str = "User";

fn auth<'a>(ctx: &ResolverContext<'a>) -> GqlResult<&'a AuthService> {
    Ok(ctx.data::<Arc<AuthService>>()?.as_ref())
}

fn role_value(r: Role) -> GqlValue {
    GqlValue::Enum(Name::new(r.as_str()))
}

fn strings(xs: &[String]) -> GqlValue {
    GqlValue::List(xs.iter().cloned().map(GqlValue::from).collect())
}

fn parse_role(v: &Value) -> Result<Option<Role>, CmsError> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) => Role::parse(s)
            .map(Some)
            .ok_or_else(|| CmsError::validation(format!("unknown role `{s}`"))),
        _ => Err(CmsError::validation("invalid role")),
    }
}

fn parse_scope(v: &Value) -> Option<Vec<String>> {
    v.as_array().map(|xs| {
        xs.iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect()
    })
}

fn user_id(ctx: &ResolverContext<'_>) -> GqlResult<uuid::Uuid> {
    let token = ctx.args.try_get("id")?.string()?;
    codec(ctx)?
        .decode(token, Some(USER_TYPE))
        .map(|(_, id)| id)
        .map_err(id_err)
}

fn types() -> Vec<TypeDef> {
    use TypeRef as T;
    let user_field = |name: &'static str, ty: TypeRef, get: fn(&User) -> Option<GqlValue>| {
        Field::new(name, ty, move |ctx| {
            FieldFuture::new(async move {
                let u = ctx.parent_value.try_downcast_ref::<User>()?;
                Ok(get(u).map(FieldValue::value))
            })
        })
    };
    let user = Object::new(USER_TYPE)
        .field(Field::new("id", T::named_nn(T::ID), |ctx| {
            FieldFuture::new(async move {
                let u = ctx.parent_value.try_downcast_ref::<User>()?;
                Ok(Some(FieldValue::value(
                    codec(&ctx)?.encode(USER_TYPE, u.id),
                )))
            })
        }))
        .field(user_field("email", T::named_nn(T::STRING), |u| {
            Some(u.email.clone().into())
        }))
        .field(user_field("name", T::named_nn(T::STRING), |u| {
            Some(u.name.clone().into())
        }))
        .field(user_field("role", T::named_nn("Role"), |u| {
            Some(role_value(u.role))
        }))
        .field(
            user_field("scope", T::named_nn_list_nn(T::STRING), |u| {
                Some(strings(&u.scope))
            })
            .description("Content types a Contributor/Reviewer works on; empty means all."),
        )
        .field(user_field("disabled", T::named_nn(T::BOOLEAN), |u| {
            Some(u.disabled.into())
        }))
        .field(user_field("createdAt", T::named_nn("DateTime"), |u| {
            Some(format_ts(u.created_at).into())
        }))
        .field(user_field("lastLoginAt", T::named("DateTime"), |u| {
            u.last_login_at.map(|t| format_ts(t).into())
        }));

    let session = Object::new("Session")
        .description("Send `token` as `Authorization: Bearer <token>`.")
        .field(Field::new("token", T::named_nn(T::STRING), |ctx| {
            FieldFuture::new(async move {
                let s = ctx.parent_value.try_downcast_ref::<Session>()?;
                Ok(Some(FieldValue::value(s.token.clone())))
            })
        }))
        .field(Field::new("expiresAt", T::named_nn("DateTime"), |ctx| {
            FieldFuture::new(async move {
                let s = ctx.parent_value.try_downcast_ref::<Session>()?;
                Ok(Some(FieldValue::value(format_ts(s.expires_at))))
            })
        }))
        .field(Field::new("user", T::named_nn(USER_TYPE), |ctx| {
            FieldFuture::new(async move {
                let s = ctx.parent_value.try_downcast_ref::<Session>()?;
                Ok(Some(FieldValue::borrowed_any(&s.user)))
            })
        }));

    let token_field = |name: &'static str, ty: TypeRef, get: fn(&ApiToken) -> Option<GqlValue>| {
        Field::new(name, ty, move |ctx| {
            FieldFuture::new(async move {
                let t = ctx.parent_value.try_downcast_ref::<ApiToken>()?;
                Ok(get(t).map(FieldValue::value))
            })
        })
    };
    let api_token = Object::new("ApiToken")
        .field(token_field("id", T::named_nn(T::STRING), |t| {
            Some(t.id.clone().into())
        }))
        .field(token_field("name", T::named_nn(T::STRING), |t| {
            Some(t.name.clone().into())
        }))
        .field(token_field("role", T::named_nn("Role"), |t| {
            Some(role_value(t.role))
        }))
        .field(token_field("scope", T::named_nn_list_nn(T::STRING), |t| {
            Some(strings(&t.scope))
        }))
        .field(token_field("createdBy", T::named_nn(T::STRING), |t| {
            Some(t.created_by.clone().into())
        }))
        .field(token_field("createdAt", T::named_nn("DateTime"), |t| {
            Some(format_ts(t.created_at).into())
        }))
        .field(token_field("expiresAt", T::named("DateTime"), |t| {
            t.expires_at.map(|e| format_ts(e).into())
        }));

    let created = Object::new("CreatedApiToken")
        .description("`token` is shown only once: store it in your secrets manager.")
        .field(Field::new("token", T::named_nn(T::STRING), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<(ApiToken, String)>()?;
                Ok(Some(FieldValue::value(c.1.clone())))
            })
        }))
        .field(Field::new("apiToken", T::named_nn("ApiToken"), |ctx| {
            FieldFuture::new(async move {
                let c = ctx.parent_value.try_downcast_ref::<(ApiToken, String)>()?;
                Ok(Some(FieldValue::borrowed_any(&c.0)))
            })
        }));

    let user_input = InputObject::new("UserInput")
        .field(InputValue::new("email", T::named_nn(T::STRING)))
        .field(InputValue::new("name", T::named_nn(T::STRING)))
        .field(InputValue::new("role", T::named_nn("Role")))
        .field(InputValue::new("scope", T::named_nn_list(T::STRING)))
        .field(InputValue::new("password", T::named_nn(T::STRING)));
    let user_update = InputObject::new("UserUpdateInput")
        .description("Omitted fields are unchanged. Role, scope, status and password changes sign the user out.")
        .field(InputValue::new("name", T::named(T::STRING)))
        .field(InputValue::new("role", T::named("Role")))
        .field(InputValue::new("scope", T::named_nn_list(T::STRING)))
        .field(InputValue::new("disabled", T::named(T::BOOLEAN)))
        .field(InputValue::new("password", T::named(T::STRING)));

    vec![
        TypeDef::Enum(Enum::new("Role").items(Role::ALL.map(|r| r.as_str()))),
        TypeDef::Object(user),
        TypeDef::Object(session),
        TypeDef::Object(api_token),
        TypeDef::Object(created),
        TypeDef::Input(user_input),
        TypeDef::Input(user_update),
    ]
}

pub(crate) fn definitions() -> (Vec<Field>, Vec<Field>, Vec<TypeDef>) {
    use TypeRef as T;

    let setup_required = Field::new("setupRequired", T::named_nn(T::BOOLEAN), |ctx| {
        FieldFuture::new(async move {
            let has = auth(&ctx)?.has_users().await.map_err(cms_err)?;
            Ok(Some(FieldValue::value(!has)))
        })
    })
    .description(
        "True until the first user exists: create the Owner with the bootstrap admin token.",
    );

    let me = Field::new("me", T::named(USER_TYPE), |ctx| {
        FieldFuture::new(async move {
            let Principal::User(actor) = principal(&ctx) else {
                return Ok(None);
            };
            let Some(id) = actor.id else { return Ok(None) };
            let user = auth(&ctx)?.get_user(id).await.map_err(cms_err)?;
            Ok(Some(FieldValue::owned_any(user)))
        })
    })
    .description("The signed-in user; null for tokens and anonymous requests.");

    let users = Field::new("users", T::named_nn_list_nn(USER_TYPE), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let all = auth(&ctx)?.list_users(&actor).await.map_err(cms_err)?;
            Ok(Some(FieldValue::list(
                all.into_iter().map(FieldValue::owned_any),
            )))
        })
    });

    let api_tokens = Field::new("apiTokens", T::named_nn_list_nn("ApiToken"), |ctx| {
        FieldFuture::new(async move {
            let actor = admin_require(&ctx)?;
            let all = auth(&ctx)?.list_api_tokens(&actor).await.map_err(cms_err)?;
            Ok(Some(FieldValue::list(
                all.into_iter().map(FieldValue::owned_any),
            )))
        })
    });

    let login = Field::new("login", T::named_nn("Session"), |ctx| {
        FieldFuture::new(async move {
            let email = ctx.args.try_get("email")?.string()?;
            let password = ctx.args.try_get("password")?.string()?;
            let session = auth(&ctx)?.login(email, password).await.map_err(cms_err)?;
            Ok(Some(FieldValue::owned_any(session)))
        })
    })
    .argument(InputValue::new("email", T::named_nn(T::STRING)))
    .argument(InputValue::new("password", T::named_nn(T::STRING)));

    let change_password = Field::new("changePassword", T::named_nn("Session"), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let current = ctx.args.try_get("currentPassword")?.string()?;
            let new = ctx.args.try_get("newPassword")?.string()?;
            let session = auth(&ctx)?
                .change_password(&actor, current, new)
                .await
                .map_err(cms_err)?;
            Ok(Some(FieldValue::owned_any(session)))
        })
    })
    .description("Signs out every other session and returns a new one.")
    .argument(InputValue::new("currentPassword", T::named_nn(T::STRING)))
    .argument(InputValue::new("newPassword", T::named_nn(T::STRING)));

    let sign_out = Field::new("signOutEverywhere", T::named_nn(T::BOOLEAN), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            auth(&ctx)?
                .sign_out_everywhere(&actor)
                .await
                .map_err(cms_err)?;
            Ok(Some(FieldValue::value(true)))
        })
    });

    let create_user = Field::new("createUser", T::named_nn(USER_TYPE), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let input = json_arg(&ctx, "input")?;
            let s = |k: &str| {
                input
                    .get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let new = NewUser {
                email: s("email"),
                name: s("name"),
                role: parse_role(&input["role"])
                    .map_err(cms_err)?
                    .unwrap_or(Role::Member),
                scope: parse_scope(&input["scope"]).unwrap_or_default(),
                password: s("password"),
            };
            let user = auth(&ctx)?
                .create_user(&actor, new)
                .await
                .map_err(cms_err)?;
            Ok(Some(FieldValue::owned_any(user)))
        })
    })
    .argument(InputValue::new("input", T::named_nn("UserInput")));

    let update_user = Field::new("updateUser", T::named_nn(USER_TYPE), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let id = user_id(&ctx)?;
            let input = json_arg(&ctx, "input")?;
            let update = UserUpdate {
                name: input.get("name").and_then(Value::as_str).map(String::from),
                role: parse_role(input.get("role").unwrap_or(&Value::Null)).map_err(cms_err)?,
                scope: input.get("scope").and_then(parse_scope),
                disabled: input.get("disabled").and_then(Value::as_bool),
                password: input
                    .get("password")
                    .and_then(Value::as_str)
                    .map(String::from),
            };
            let user = auth(&ctx)?
                .update_user(&actor, id, update)
                .await
                .map_err(cms_err)?;
            Ok(Some(FieldValue::owned_any(user)))
        })
    })
    .argument(InputValue::new("id", T::named_nn(T::ID)))
    .argument(InputValue::new("input", T::named_nn("UserUpdateInput")));

    let delete_user = Field::new("deleteUser", T::named_nn(T::ID), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let id = user_id(&ctx)?;
            auth(&ctx)?.delete_user(&actor, id).await.map_err(cms_err)?;
            Ok(Some(FieldValue::value(
                ctx.args.try_get("id")?.string()?.to_string(),
            )))
        })
    })
    .argument(InputValue::new("id", T::named_nn(T::ID)));

    let create_token = Field::new("createApiToken", T::named_nn("CreatedApiToken"), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let name = ctx.args.try_get("name")?.string()?;
            let role = Role::parse(ctx.args.try_get("role")?.enum_name()?).unwrap_or(Role::Member);
            let scope = parse_scope(&json_arg(&ctx, "scope")?).unwrap_or_default();
            let expires = match opt_arg(&ctx, "expiresInDays") {
                Some(v) => {
                    let days = v.i64()?;
                    if !(1..=3650).contains(&days) {
                        return Err(cms_err(CmsError::validation(
                            "expiresInDays must be between 1 and 3650",
                        )));
                    }
                    Some(Duration::from_secs(days as u64 * 86_400))
                }
                None => None,
            };
            let created = auth(&ctx)?
                .create_api_token(&actor, name, role, scope, expires)
                .await
                .map_err(cms_err)?;
            Ok(Some(FieldValue::owned_any(created)))
        })
    })
    .argument(InputValue::new("name", T::named_nn(T::STRING)))
    .argument(InputValue::new("role", T::named_nn("Role")))
    .argument(InputValue::new("scope", T::named_nn_list(T::STRING)))
    .argument(InputValue::new("expiresInDays", T::named(T::INT)));

    let revoke_token = Field::new("revokeApiToken", T::named_nn(T::STRING), |ctx| {
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let id = ctx.args.try_get("id")?.string()?.to_string();
            auth(&ctx)?
                .revoke_api_token(&actor, &id)
                .await
                .map_err(cms_err)?;
            Ok(Some(FieldValue::value(id)))
        })
    })
    .argument(InputValue::new("id", T::named_nn(T::STRING)));

    (
        vec![setup_required, me, users, api_tokens],
        vec![
            login,
            change_password,
            sign_out,
            create_user,
            update_user,
            delete_user,
            create_token,
            revoke_token,
        ],
        types(),
    )
}
