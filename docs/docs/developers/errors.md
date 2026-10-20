---
sidebar_position: 9
title: Errors
---

# Errors

GraphQL errors come back in `errors[]`, with a machine-readable code in
`extensions.code`:

```json
{
  "data": null,
  "errors": [{ "message": "you cannot edit `Page` entries", "extensions": { "code": "FORBIDDEN" } }]
}
```

| Code | Typical causes |
|---|---|
| `FORBIDDEN` | not signed in for a non-public operation, role or scope not allowed, wrong password at login, account locked |
| `BAD_USER_INPUT` | validation (missing required field, wrong type, unknown field, bad enum value, invalid id), a plugin rejected the write, request changes without a comment |
| `NOT_FOUND` | the entry, user, plugin or content type doesn't exist |
| `CONFLICT` | duplicate content type or email, generated names would collide, deleting a referenced type, workflow transition not allowed in the current status, removing the last Owner |
| `ID_EXPIRED` | the `id` is older than `TESS_ID_TTL`: refetch the entry |
| `PLUGIN_ERROR` | a plugin crashed, timed out or returned invalid output |
| `INTERNAL_SERVER_ERROR` | storage failure; details are only in the server logs |

Unknown fields or arguments (for example a content type that doesn't exist
yet) are reported by GraphQL validation, without a code.

## HTTP status codes

| Status | Meaning |
|---|---|
| `200` | GraphQL response, including errors in `errors[]` |
| `401` | invalid, expired or revoked credentials: sign in again |
| `503` | the server can't reach its storage to check credentials (and `/health` when the database is down) |

## REST endpoints

`POST /media/upload` and `POST /api/plugins/{name}` answer with JSON
`{"error": "…", "code": "…"}` and a matching status: `400 BAD_USER_INPUT`,
`401 UNAUTHENTICATED`, `403 FORBIDDEN`, `404 NOT_FOUND`, `409 CONFLICT`,
`422 PLUGIN_ERROR`, `500 INTERNAL_SERVER_ERROR`.
