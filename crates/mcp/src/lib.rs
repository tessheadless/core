// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Model Context Protocol server for AI agents.
//!
//! It exposes two generic tools, so the tool list does not change when content
//! models do:
//!
//! - `get_schema`: the GraphQL SDL, or one type of it.
//! - `graphql_query`: runs a query or mutation and returns `data` and `errors`.
//!
//! Two transports share the same tools:
//!
//! - [`http_service`] runs in-process inside the server (`/mcp`). The caller's
//!   [`Principal`] is read from the HTTP request extensions, which the server
//!   sets after authenticating the bearer token. Access rules are therefore the
//!   ones of `/graphql`.
//! - [`run_stdio`] (`tess mcp`) is a local proxy for desktop agents. It sends
//!   every call to a running server with the `TESS_MCP_TOKEN` credential, and
//!   never touches the database.

use std::sync::Arc;

use async_graphql::Variables;
use http::request::Parts;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, ServerHandler, ServiceExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tess_graphql::{GraphqlService, Principal};

/// Largest GraphQL query accepted from an agent.
pub const MAX_QUERY_BYTES: usize = 32 * 1024;
/// Largest result returned to an agent. Larger results are refused with a hint.
pub const MAX_RESULT_BYTES: usize = 256 * 1024;

const SCHEMA_PATH: &str = "/graphql/schema.graphql";
const GRAPHQL_PATH: &str = "/graphql";

/// How tools reach the GraphQL API.
#[derive(Clone)]
pub enum Backend {
    /// The server's own service, in the same process.
    Local(Arc<GraphqlService>),
    /// A running server, over HTTP.
    Remote(RemoteApi),
}

/// Connection to a running server, used by the stdio proxy.
#[derive(Clone)]
pub struct RemoteApi {
    base: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl RemoteApi {
    /// `base` is the server origin, for example `https://cms.example.com`.
    pub fn new(base: impl Into<String>, token: Option<String>) -> Self {
        RemoteApi {
            base: base.into().trim_end_matches('/').to_string(),
            token,
            client: reqwest::Client::new(),
        }
    }

    fn request(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }
}

impl Backend {
    async fn execute(
        &self,
        query: String,
        variables: Option<Value>,
        operation_name: Option<String>,
        principal: Principal,
    ) -> Result<Value, String> {
        match self {
            Backend::Local(service) => {
                let mut request = async_graphql::Request::new(query);
                if let Some(vars) = variables {
                    request = request.variables(Variables::from_json(vars));
                }
                if let Some(op) = operation_name {
                    request = request.operation_name(op);
                }
                let response = service.execute(request, principal).await;
                serde_json::to_value(&response).map_err(|e| e.to_string())
            }
            Backend::Remote(api) => {
                let mut body = json!({ "query": query });
                if let Some(vars) = variables {
                    body["variables"] = vars;
                }
                if let Some(op) = operation_name {
                    body["operationName"] = json!(op);
                }
                let url = format!("{}{GRAPHQL_PATH}", api.base);
                let response = api
                    .request(api.client.post(url).json(&body))
                    .send()
                    .await
                    .map_err(|e| format!("cannot reach the GraphQL endpoint: {e}"))?;
                response
                    .json::<Value>()
                    .await
                    .map_err(|e| format!("invalid response from the server: {e}"))
            }
        }
    }

    async fn sdl(&self) -> Result<String, String> {
        match self {
            Backend::Local(service) => service.sdl().await.map_err(|e| e.to_string()),
            Backend::Remote(api) => {
                let url = format!("{}{SCHEMA_PATH}", api.base);
                let response = api
                    .request(api.client.get(url))
                    .send()
                    .await
                    .and_then(reqwest::Response::error_for_status)
                    .map_err(|e| format!("cannot fetch the schema: {e}"))?;
                response
                    .text()
                    .await
                    .map_err(|e| format!("cannot read the schema: {e}"))
            }
        }
    }
}

/// The MCP server handler. Cheap to clone.
#[derive(Clone)]
pub struct GraphqlTools {
    backend: Backend,
}

impl GraphqlTools {
    pub fn new(backend: Backend) -> Self {
        GraphqlTools { backend }
    }

    async fn get_schema(&self, args: Value) -> CallToolResult {
        let args: SchemaArgs = match serde_json::from_value(args) {
            Ok(a) => a,
            Err(e) => return failure(format!("invalid arguments: {e}")),
        };
        let sdl = match self.backend.sdl().await {
            Ok(sdl) => sdl,
            Err(e) => return failure(e),
        };
        match args.type_name {
            Some(name) => match type_definition(&sdl, &name) {
                Some(definition) => success(definition),
                None => failure(format!(
                    "no type named `{name}`. Available types: {}",
                    definition_names(&sdl).join(", ")
                )),
            },
            None if sdl.len() > MAX_RESULT_BYTES => failure(format!(
                "the schema is {} bytes, over the {MAX_RESULT_BYTES}-byte limit. Pass `type` to read one type at a time.",
                sdl.len()
            )),
            None => success(sdl),
        }
    }

    async fn graphql_query(&self, args: Value, principal: Principal) -> CallToolResult {
        let args: QueryArgs = match serde_json::from_value(args) {
            Ok(a) => a,
            Err(e) => return failure(format!("invalid arguments: {e}")),
        };
        if args.query.len() > MAX_QUERY_BYTES {
            return failure(format!(
                "the query is over the {MAX_QUERY_BYTES}-byte limit"
            ));
        }
        if args.variables.as_ref().is_some_and(|v| !v.is_object()) {
            return failure("`variables` must be a JSON object".to_string());
        }
        let response = match self
            .backend
            .execute(args.query, args.variables, args.operation_name, principal)
            .await
        {
            Ok(v) => v,
            Err(e) => return failure(e),
        };
        let has_errors = response.get("errors").is_some_and(|e| !e.is_null());
        let text = response.to_string();
        if text.len() > MAX_RESULT_BYTES {
            return failure(format!(
                "the result is {} bytes, over the {MAX_RESULT_BYTES}-byte limit. Select fewer fields or use a smaller `first`.",
                text.len()
            ));
        }
        if has_errors {
            failure(text)
        } else {
            success(text)
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryArgs {
    query: String,
    #[serde(default)]
    variables: Option<Value>,
    #[serde(default)]
    operation_name: Option<String>,
}

#[derive(Deserialize, Default)]
struct SchemaArgs {
    #[serde(default, rename = "type")]
    type_name: Option<String>,
}

fn success(text: String) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(text)])
}

fn failure(text: String) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(text)])
}

/// The principal set by the HTTP layer. Anonymous when absent (fails closed).
fn principal_of(context: &RequestContext<RoleServer>) -> Principal {
    context
        .extensions
        .get::<Parts>()
        .and_then(|parts| parts.extensions.get::<Principal>())
        .cloned()
        .unwrap_or(Principal::Anonymous)
}

fn object(value: Value) -> Arc<JsonObject> {
    Arc::new(
        value
            .as_object()
            .cloned()
            .expect("tool schemas are objects"),
    )
}

fn tool_list() -> Vec<Tool> {
    vec![
        Tool::new(
            "get_schema",
            "Read the GraphQL schema of this CMS. Call it before writing a query: it lists the content types \
             (e.g. `Post`), their fields, the list operations (`posts`, `post`), and the create, update, \
             publish and delete mutations. Without `type`, returns the whole schema. With `type`, returns only \
             that type's definition.",
            object(json!({
                "type": "object",
                "properties": {
                    "type": {
                        "type": "string",
                        "description": "Optional type name, such as `Query`, `Post` or `PostCreateInput`."
                    }
                },
                "additionalProperties": false
            })),
        ),
        Tool::new(
            "graphql_query",
            "Run a GraphQL query or mutation against the CMS and return the JSON response (`data` and `errors`). \
             Anonymous callers read published content only. Drafts (`stage: DRAFT`), writes and other protected \
             operations need an API token, and the token's role decides what is allowed. Ids are opaque and \
             expire, so fetch them in the same session before using them in a mutation.",
            object(json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The GraphQL document: a query or a mutation."
                    },
                    "variables": {
                        "type": "object",
                        "description": "Values for the variables declared in the document."
                    },
                    "operationName": {
                        "type": "string",
                        "description": "Which operation to run, when the document contains several."
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            })),
        ),
    ]
}

impl ServerHandler for GraphqlTools {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("tess", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Tess headless CMS. Use get_schema to discover content types and operations, then graphql_query \
                 to read or change content.",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(tool_list()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = Value::Object(request.arguments.unwrap_or_default());
        let result = match request.name.as_ref() {
            "get_schema" => self.get_schema(args).await,
            "graphql_query" => self.graphql_query(args, principal_of(&context)).await,
            other => {
                return Err(ErrorData::invalid_params(
                    format!("unknown tool `{other}`"),
                    None,
                ));
            }
        };
        Ok(result.into())
    }
}

/// Streamable HTTP service for the in-process `/mcp` endpoint. Stateless: each
/// request is self-contained, so it works across instances and on Lambda.
/// The caller must put the authenticated [`Principal`] in the request
/// extensions. `allowed_hosts` is the list of `Host` values accepted.
pub fn http_service(
    service: Arc<GraphqlService>,
    allowed_hosts: Vec<String>,
) -> StreamableHttpService<GraphqlTools, NeverSessionManager> {
    let backend = Backend::Local(service);
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_allowed_hosts(allowed_hosts);
    StreamableHttpService::new(
        move || Ok(GraphqlTools::new(backend.clone())),
        Arc::new(NeverSessionManager::default()),
        config,
    )
}

/// Serves MCP over stdin and stdout until the client disconnects.
pub async fn run_stdio(api: RemoteApi) -> anyhow::Result<()> {
    let server = GraphqlTools::new(Backend::Remote(api))
        .serve(rmcp::transport::stdio())
        .await?;
    server.waiting().await?;
    Ok(())
}

/// Names of the top-level definitions of an SDL document, in order.
fn definition_names(sdl: &str) -> Vec<&str> {
    sdl.lines().filter_map(definition_name).collect()
}

/// The name of a top-level `type`, `input`, `enum`, `union`, `interface` or `scalar` line.
fn definition_name(line: &str) -> Option<&str> {
    if line.starts_with(char::is_whitespace) {
        return None;
    }
    let mut parts = line.split_whitespace();
    match parts.next()? {
        "type" | "input" | "enum" | "union" | "interface" | "scalar" => {}
        _ => return None,
    }
    Some(parts.next()?.trim_end_matches('{'))
}

/// The SDL of one top-level definition, from its line to its closing `}`.
fn type_definition(sdl: &str, name: &str) -> Option<String> {
    let lines: Vec<&str> = sdl.lines().collect();
    let start = lines
        .iter()
        .position(|l| definition_name(l) == Some(name))?;
    let mut out = Vec::new();
    if lines[start].trim_end().ends_with('{') {
        for line in &lines[start..] {
            out.push(*line);
            if *line == "}" {
                break;
            }
        }
    } else {
        out.push(lines[start]);
    }
    Some(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SDL: &str = "type Post {\n  title: String!\n  type: String\n}\n\ninput PostInput {\n  title: String\n}\n\nscalar Date\n";

    #[test]
    fn finds_one_type_only() {
        assert_eq!(
            type_definition(SDL, "Post").unwrap(),
            "type Post {\n  title: String!\n  type: String\n}"
        );
        assert_eq!(
            type_definition(SDL, "PostInput").unwrap(),
            "input PostInput {\n  title: String\n}"
        );
        assert_eq!(type_definition(SDL, "Date").unwrap(), "scalar Date");
        assert!(type_definition(SDL, "Missing").is_none());
    }

    #[test]
    fn field_named_type_is_not_a_definition() {
        assert_eq!(definition_names(SDL), vec!["Post", "PostInput", "Date"]);
    }
}
