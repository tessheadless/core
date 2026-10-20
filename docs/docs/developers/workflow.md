---
sidebar_position: 5
title: Draft, publish and review
---

# Draft, publish and review

## Draft and publish

Every entry has a **draft** and, once published, a **published** version.

- `createPost`/`updatePost` change the draft only.
- `publishPost(id)` copies the draft to the published version.
- `unpublishPost(id)` removes the published version; the draft stays.
- Queries read `PUBLISHED` by default. Pass `stage: DRAFT` to preview drafts
  (signed-in users only).

## The review workflow

Admins enable it per content type (`workflow: true`, or the toggle in the
admin UI). It's typically used for pages and blog posts.

```text
DRAFT ──submit──▶ IN_REVIEW ──approve──▶ PUBLISHED
  ▲                   │
  └─ CHANGES_REQUESTED ◀── request changes (comment required)
```

| Mutation | Who | Effect |
|---|---|---|
| `submitPostForReview(id)` | Contributor, Admin, Owner | `DRAFT` or `CHANGES_REQUESTED` → `IN_REVIEW`. The draft must be valid. |
| `approvePost(id, comment)` | Reviewer, Admin, Owner | `IN_REVIEW` → published (`PUBLISHED`) |
| `requestChangesPost(id, comment: String!)` | Reviewer, Admin, Owner | `IN_REVIEW` → `CHANGES_REQUESTED` |
| `publishPost(id)` | Admin, Owner | publish directly, bypassing review |

- **Contributors** can't publish workflow types directly. On types
  *without* workflow, they publish themselves.
- **Editing** an entry (any `updatePost`) sets it back to `DRAFT`, also while
  it's in review. Reviewers therefore always approve exactly what gets
  published.

### Status, authors and history

```graphql
{
  posts(stage: DRAFT, where: { workflowStatus: IN_REVIEW }) {
    items {
      title
      workflowStatus
      createdBy
      updatedBy
      history { action at by comment }   # newest first
    }
  }
}
```

History actions: `CREATED`, `UPDATED`, `SUBMITTED`, `CHANGES_REQUESTED`,
`APPROVED`, `PUBLISHED`, `UNPUBLISHED`. The last 50 events are kept.
`createdBy`, `updatedBy` and `history` contain staff names, so they are
`null` for anonymous readers.

Plugins can react to `submit`, `approve` and `requestChanges`, for example to
notify reviewers. See the [plugin contract](../plugins/contract.md).
