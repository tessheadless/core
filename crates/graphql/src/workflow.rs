// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Review workflow in the API: entry metadata fields and the
//! `submitXForReview` / `approveX` / `requestChangesX` mutations.

use std::sync::Arc;

use async_graphql::dynamic::{Enum, Field, FieldFuture, FieldValue, InputValue, Object, TypeRef};
use async_graphql::{Name, Value as GqlValue};
use tess_core::model::format_ts;
use tess_core::{Cms, ContentType, HistoryAction, HistoryEvent, Stage, WorkflowStatus};

use crate::Principal;
use crate::naming::Names;
use crate::schema::{EntryNode, cms_err, id_arg, opt_arg, principal, require_write};

const STATUSES: [WorkflowStatus; 4] =
    [WorkflowStatus::Draft, WorkflowStatus::InReview, WorkflowStatus::ChangesRequested, WorkflowStatus::Published];

fn action_name(a: HistoryAction) -> &'static str {
    match a {
        HistoryAction::Created => "CREATED",
        HistoryAction::Updated => "UPDATED",
        HistoryAction::Submitted => "SUBMITTED",
        HistoryAction::ChangesRequested => "CHANGES_REQUESTED",
        HistoryAction::Approved => "APPROVED",
        HistoryAction::Published => "PUBLISHED",
        HistoryAction::Unpublished => "UNPUBLISHED",
    }
}

pub(crate) fn status_enum() -> Enum {
    Enum::new("WorkflowStatus")
        .description("State of an entry's draft: DRAFT → IN_REVIEW → PUBLISHED, or back to CHANGES_REQUESTED.")
        .items(STATUSES.map(|s| s.as_str()))
}

pub(crate) fn history_action_enum() -> Enum {
    Enum::new("HistoryAction").items(
        [
            HistoryAction::Created,
            HistoryAction::Updated,
            HistoryAction::Submitted,
            HistoryAction::ChangesRequested,
            HistoryAction::Approved,
            HistoryAction::Published,
            HistoryAction::Unpublished,
        ]
        .map(action_name),
    )
}

fn event_field(name: &'static str, ty: TypeRef, get: fn(&HistoryEvent) -> Option<GqlValue>) -> Field {
    Field::new(name, ty, move |ctx| {
        FieldFuture::new(async move {
            let e = ctx.parent_value.try_downcast_ref::<HistoryEvent>()?;
            Ok(get(e).map(FieldValue::value))
        })
    })
}

pub(crate) fn history_event_type() -> Object {
    Object::new("HistoryEvent")
        .field(event_field("action", TypeRef::named_nn("HistoryAction"), |e| {
            Some(GqlValue::Enum(Name::new(action_name(e.action))))
        }))
        .field(event_field("at", TypeRef::named_nn("DateTime"), |e| Some(format_ts(e.at).into())))
        .field(event_field("by", TypeRef::named_nn(TypeRef::STRING), |e| Some(e.by.name.clone().into())))
        .field(event_field("comment", TypeRef::named(TypeRef::STRING), |e| e.comment.clone().map(Into::into)))
}

/// Editorial metadata is only shown to signed-in principals: public readers
/// see `null` (staff names stay private).
fn staff_only(ctx: &async_graphql::dynamic::ResolverContext<'_>) -> bool {
    matches!(principal(ctx), Principal::User(_))
}

pub(crate) fn entry_fields() -> Vec<Field> {
    let author = |name: &'static str, pick: fn(&tess_core::Entry) -> Option<String>| {
        Field::new(name, TypeRef::named(TypeRef::STRING), move |ctx| {
            FieldFuture::new(async move {
                let node = ctx.parent_value.try_downcast_ref::<EntryNode>()?;
                if !staff_only(&ctx) {
                    return Ok(None);
                }
                Ok(pick(&node.entry).map(FieldValue::value))
            })
        })
        .description("Name of the user (or token); null for public readers.")
    };
    vec![
        Field::new("workflowStatus", TypeRef::named_nn("WorkflowStatus"), |ctx| {
            FieldFuture::new(async move {
                let node = ctx.parent_value.try_downcast_ref::<EntryNode>()?;
                Ok(Some(FieldValue::value(GqlValue::Enum(Name::new(node.entry.status.as_str())))))
            })
        }),
        author("createdBy", |e| e.created_by.as_ref().map(|a| a.name.clone())),
        author("updatedBy", |e| e.updated_by.as_ref().map(|a| a.name.clone())),
        Field::new("history", TypeRef::named_nn_list("HistoryEvent"), |ctx| {
            FieldFuture::new(async move {
                let node = ctx.parent_value.try_downcast_ref::<EntryNode>()?;
                if !staff_only(&ctx) {
                    return Ok(None);
                }
                Ok(Some(FieldValue::list(node.entry.history.iter().rev().map(|h| FieldValue::borrowed_any(h)))))
            })
        })
        .description("Most recent first; null for public readers."),
    ]
}

pub(crate) fn mutations(ct: &Arc<ContentType>, n: &Names) -> Vec<Field> {
    let id_input = || InputValue::new("id", TypeRef::named_nn(TypeRef::ID));

    let a = ct.api_id.clone();
    let submit = Field::new(&n.submit, TypeRef::named(&n.object), move |ctx| {
        let a = a.clone();
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let id = id_arg(&ctx, &a)?;
            let entry = ctx.data::<Cms>()?.submit_for_review(&a, id, &actor).await.map_err(cms_err)?;
            Ok(EntryNode::value(entry, Stage::Draft))
        })
    })
    .description("Send the draft to reviewers.")
    .argument(id_input());

    let a = ct.api_id.clone();
    let approve = Field::new(&n.approve, TypeRef::named(&n.object), move |ctx| {
        let a = a.clone();
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let id = id_arg(&ctx, &a)?;
            let comment = opt_arg(&ctx, "comment").map(|v| v.string().map(String::from)).transpose()?;
            let entry = ctx.data::<Cms>()?.approve(&a, id, comment, &actor).await.map_err(cms_err)?;
            Ok(EntryNode::value(entry, Stage::Published))
        })
    })
    .description("Approve an entry in review: it is published.")
    .argument(id_input())
    .argument(InputValue::new("comment", TypeRef::named(TypeRef::STRING)));

    let a = ct.api_id.clone();
    let request_changes = Field::new(&n.request_changes, TypeRef::named(&n.object), move |ctx| {
        let a = a.clone();
        FieldFuture::new(async move {
            let actor = require_write(&ctx)?;
            let id = id_arg(&ctx, &a)?;
            let comment = ctx.args.try_get("comment")?.string()?.to_string();
            let entry = ctx.data::<Cms>()?.request_changes(&a, id, comment, &actor).await.map_err(cms_err)?;
            Ok(EntryNode::value(entry, Stage::Draft))
        })
    })
    .description("Send an entry in review back to its authors, explaining what to change.")
    .argument(id_input())
    .argument(InputValue::new("comment", TypeRef::named_nn(TypeRef::STRING)));

    vec![submit, approve, request_changes]
}
