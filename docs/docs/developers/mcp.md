---
sidebar_position: 11
title: MCP server for AI agents
---

# MCP server for AI agents

Tess includes a [Model Context Protocol](https://modelcontextprotocol.io)
server, so AI agents (Claude, Cursor, and other MCP clients) can read and
write content through the same GraphQL API as your applications.

The server exposes two generic tools, so it works with any content model
without reconfiguration:

| Tool | Purpose |
|---|---|
| `get_schema` | Returns the GraphQL schema. Pass `type` (for example `Post`) to read one type only. Agents call it first to discover content types and operations. |
| `graphql_query` | Runs a query or mutation with optional `variables` and `operationName`. Returns the JSON response, with `data` and `errors`. |

Agents can do everything the GraphQL API allows for the credential they use:
the rules in [Authentication, roles and API tokens](authentication.md) apply
unchanged.

## Enabling the endpoint

The endpoint is **off by default**. Turn it on in the server environment:

```bash
TESS_MCP=true
# Host headers accepted on /mcp. Add your public hostname in production.
TESS_MCP_ALLOWED_HOSTS=localhost,127.0.0.1,::1,cms.example.com
```

With `TESS_MCP=true`, the endpoint is served at `https://<your-host>/mcp`.
It uses the stateless Streamable HTTP transport, so it works on several
instances and on AWS Lambda.

## Connecting an agent

There are two ways to connect. Both use the same tools and the same access
rules.

### Remote (HTTP)

Use this when the client supports remote MCP servers. Send an API token as a
bearer credential:

```json
{
  "mcpServers": {
    "tess": {
      "type": "http",
      "url": "https://cms.example.com/mcp",
      "headers": { "Authorization": "Bearer tess_t1_…" }
    }
  }
}
```

Without an `Authorization` header, the agent is anonymous and can read
published content only, if `TESS_PUBLIC_READ` is enabled.

### Local (stdio)

Use this for clients that only launch local programs. The `tess mcp`
subcommand is a small proxy: it reads from stdin and writes to stdout, and
forwards each call to a running Tess server. It needs no database, and none
of the server secrets.

```json
{
  "mcpServers": {
    "tess": {
      "command": "tess",
      "args": ["mcp"],
      "env": {
        "TESS_MCP_URL": "https://cms.example.com",
        "TESS_MCP_TOKEN": "tess_t1_…"
      }
    }
  }
}
```

`TESS_MCP_URL` defaults to `http://localhost:8080`. `TESS_MCP_TOKEN` is
optional: without it, the agent is anonymous.

## Choosing a token for agents

Create a dedicated API token for each agent, with the lowest role that does
the job. Admins create tokens with `createApiToken`, and an expiry keeps a
leaked token short-lived:

| Agent does | Suggested role |
|---|---|
| Reads drafts and published content | `MEMBER` |
| Creates and edits drafts | `CONTRIBUTOR` (limit it with `scope`) |
| Approves workflow reviews | `REVIEWER` |

Revoke a token with `revokeApiToken` if it is no longer needed.

## Safety

- **Mutations are allowed, but limited by role.** A `MEMBER` token can read
  drafts but cannot write. Restrict agents with `scope` to the content types
  they should touch.
- **Query limits still apply.** Maximum depth (`TESS_MAX_QUERY_DEPTH`) and
  page size (`TESS_MAX_PAGE_SIZE`) are enforced by the GraphQL schema.
- **Size limits.** A query is refused above 32 KiB, and a result above 256 KiB.
  The agent receives an error explaining how to narrow the request.
- **Ids expire.** Fetch an entry again before using its `id` in a mutation
  (see [IDs and refs](ids-and-refs.md)).

## Troubleshooting

- **401 on `/mcp`**: the token is invalid, expired or revoked.
- **403 with "Host header is not allowed"**: add the hostname to
  `TESS_MCP_ALLOWED_HOSTS`.
- **`isError: true` with `FORBIDDEN`**: the token's role or scope does not
  allow the operation.
- **`cannot reach the GraphQL endpoint`** (stdio): check `TESS_MCP_URL` and
  that the server is running.
