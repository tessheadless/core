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

## The API token: `Authorization: Bearer`

Every client authenticates the same way: it sends an **API token** in the
`Authorization` header of each request to `/mcp`:

```http
Authorization: Bearer tess_t1_…
```

- **Create one token per agent** in the admin (**API tokens**) or with
  `createApiToken`. The token is shown once: copy it into an environment
  variable (here `TESS_MCP_TOKEN`), not into a file you might commit.
- **Its role is the agent's role.** A Contributor token writes drafts but
  can't change models; an Admin token can. See
  [Choosing a token for agents](#choosing-a-token-for-agents).
- **Without the header**, the agent is anonymous: it reads published content
  only, and only if `TESS_PUBLIC_READ` is enabled.
- **An invalid, expired or revoked token** gets a `401`, and the client
  reports the server as failed.

Write `Bearer`, a space, then the token. Most clients read the token from an
environment variable (the syntax differs per client, see below), so set it
in the shell or service that starts the client:

```bash
export TESS_MCP_TOKEN=tess_t1_…
```

## Connecting an agent

There are two ways to connect, with the same tools and access rules:

- **Remote (HTTP)**: the client calls `https://<your-host>/mcp` directly and
  sends the token in the `Authorization` header. Use it whenever the client
  supports remote servers with custom headers.
- **Local (stdio)**: the client starts `tess mcp`, a small local proxy that
  forwards each call to your server with the token. Use it for clients that
  only start local programs. It needs the `tess` binary on that machine (from
  the [release archives](https://github.com/tessheadless/core/releases)), but
  no database and none of the server's secrets.

| Client | How | Token |
|---|---|---|
| [Claude Code](#claude-code) | Remote | `--header "Authorization: Bearer …"` or `${TESS_MCP_TOKEN}` in `.mcp.json` |
| [Claude Desktop](#claude-desktop) | Local (`tess mcp`) | `TESS_MCP_TOKEN` in `env` |
| [OpenCode](#opencode) | Remote | `"Authorization": "Bearer {env:TESS_MCP_TOKEN}"` |
| [Codex](#codex) | Remote | `bearer_token_env_var = "TESS_MCP_TOKEN"` |
| [Pi](#pi) | Remote | `"Authorization": "Bearer ${TESS_MCP_TOKEN}"` |
| [Others](#other-clients) | Remote or local | an `Authorization` header, or `TESS_MCP_TOKEN` for `tess mcp` |

In the examples, replace `https://cms.example.com` with your Tess URL.

### Claude Code

Add the server with the CLI (`--scope user` makes it available in every
project):

```bash
claude mcp add --transport http tess https://cms.example.com/mcp \
  --header "Authorization: Bearer ${TESS_MCP_TOKEN}" --scope user
```

Or share it with your team in the project's `.mcp.json`, with the token read
from each person's environment (`${TESS_MCP_TOKEN}` is expanded by Claude
Code, so the file holds no secret):

```json
{
  "mcpServers": {
    "tess": {
      "type": "http",
      "url": "https://cms.example.com/mcp",
      "headers": { "Authorization": "Bearer ${TESS_MCP_TOKEN}" }
    }
  }
}
```

Check it with `/mcp` in a session. Use your own variable name: Claude Code
doesn't expand its own credential variables (such as `ANTHROPIC_API_KEY`) in
headers.

### Claude Desktop

Claude Desktop's connectors sign in with OAuth and can't send a fixed
`Authorization` header, so use the local proxy. In **Settings → Developer →
Edit Config**, add Tess to `claude_desktop_config.json`
(`~/Library/Application Support/Claude/` on macOS,
`%APPDATA%\Claude\` on Windows), then restart Claude Desktop:

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

`tess mcp` sends `TESS_MCP_TOKEN` as the `Authorization: Bearer` header. Use
the full path of the binary in `command` if it isn't on the `PATH`
(`"/usr/local/bin/tess"`, or `"C:\\Tess\\tess.exe"` on Windows: backslashes are doubled in JSON).

### OpenCode

In `opencode.json` (in the project, or the global OpenCode config), with
`oauth: false` so OpenCode doesn't try to sign in with OAuth:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "tess": {
      "type": "remote",
      "url": "https://cms.example.com/mcp",
      "enabled": true,
      "oauth": false,
      "headers": { "Authorization": "Bearer {env:TESS_MCP_TOKEN}" }
    }
  }
}
```

`{env:TESS_MCP_TOKEN}` is replaced with the environment variable. Check the
connection with `opencode mcp debug tess`.

### Codex

In `~/.codex/config.toml` (or `.codex/config.toml` in a trusted project).
Codex reads the token from the variable named in `bearer_token_env_var` and
sends it as `Authorization: Bearer …`:

```toml
[mcp_servers.tess]
url = "https://cms.example.com/mcp"
bearer_token_env_var = "TESS_MCP_TOKEN"
```

Start Codex from a shell where `TESS_MCP_TOKEN` is set, and check with
`codex mcp list`.

### Pi

Pi supports MCP from version 1.0. Add Tess to `~/.pi/agent/mcp.json` (or
`.pi/mcp.json` in a trusted project); `${TESS_MCP_TOKEN}` is read from the
environment:

```json
{
  "mcpServers": {
    "tess": {
      "url": "https://cms.example.com/mcp",
      "headers": { "Authorization": "Bearer ${TESS_MCP_TOKEN}" },
      "description": "Read and edit the website content in Tess"
    }
  }
}
```

Check it with `pi mcp list`, or `/mcp` in a session (`/reload` after editing
the file). With an MCP extension such as `pi-mcp-adapter` installed, Pi uses
the extension's configuration instead: see its documentation.

### Other clients

Any MCP client works. With remote servers and custom headers, use the URL and
the `Authorization: Bearer` header, as above. Otherwise, start the local
proxy: command `tess`, arguments `mcp`, environment `TESS_MCP_URL` (default
`http://localhost:8080`) and `TESS_MCP_TOKEN`.

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

- **401 on `/mcp`**: the token is invalid, expired or revoked. Check the
  header reads `Bearer`, a space, then the token, and that the environment
  variable is set where the client runs: an unset variable sends `Bearer `
  alone, or the variable's name.
- **The client opens a sign-in page or asks for OAuth**: Tess uses API
  tokens, not OAuth. Send the `Authorization` header (OpenCode: `"oauth": false`),
  or use the local `tess mcp` proxy.
- **Only published content, and "sign in required" on writes**: no token
  reached Tess, so the agent is anonymous.
- **403 with "Host header is not allowed"**: add the hostname to
  `TESS_MCP_ALLOWED_HOSTS`.
- **`isError: true` with `FORBIDDEN`**: the token's role or scope does not
  allow the operation.
- **`cannot reach the GraphQL endpoint`** (stdio): check `TESS_MCP_URL` and
  that the server is running.
