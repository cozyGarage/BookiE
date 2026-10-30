use std::sync::Arc;

use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult, PaginatedRequestParams,
        ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
};
use serde_json::{Map, Value as JsonValue};

use crate::bridge::McpBridge;
use crate::server::{tool_annotations, tool_description, tool_schema};
use crate::tools::{self, TOOL_NAMES};

#[derive(Clone)]
pub struct BridgeHandler {
    bridge: Arc<McpBridge>,
}

impl BridgeHandler {
    pub fn new(bridge: Arc<McpBridge>) -> Self {
        Self { bridge }
    }
}

fn token_from_meta(context: &RequestContext<RoleServer>) -> Option<String> {
    context
        .meta
        .0
        .get("authorization")
        .and_then(JsonValue::as_str)
        .map(str::to_string)
}

fn json_object(value: JsonValue) -> Map<String, JsonValue> {
    match value {
        JsonValue::Object(map) => map,
        _ => Map::new(),
    }
}

fn tool_error_result(error: &crate::errors::McpToolError) -> CallToolResult {
    let mut result = CallToolResult::error(vec![ContentBlock::text(error.message)]);
    result.structured_content = Some(error.structured_content());
    result
}

impl ServerHandler for BridgeHandler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let token = token_from_meta(&context)
            .ok_or_else(|| ErrorData::invalid_request("missing token (pass _meta.authorization)", None))?;
        self.bridge
            .authenticate(&token)
            .map_err(|message| ErrorData::invalid_request(message, None))?;
        let tools = TOOL_NAMES
            .iter()
            .map(|name| {
                let tool = Tool::new(*name, tool_description(name), Arc::new(json_object(tool_schema(name))));
                match serde_json::from_value(tool_annotations(name)) {
                    Ok(annotations) => tool.with_annotations(annotations),
                    Err(_) => tool,
                }
            })
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let mut arguments = JsonValue::Object(request.arguments.unwrap_or_default());
        let token = token_from_meta(&context)
            .or_else(|| arguments.get("token").and_then(JsonValue::as_str).map(str::to_string));
        let outcome = match token {
            None => Err(crate::errors::McpToolError::from("missing token")),
            Some(token) => match self.bridge.authenticate(&token) {
                Err(message) => Err(crate::errors::McpToolError::from(message)),
                Ok(token) => {
                    if let Some(map) = arguments.as_object_mut() {
                        map.remove("token");
                    }
                    tools::dispatch_structured(&self.bridge, &token, &request.name, arguments).await
                }
            },
        };
        let result = match outcome {
            Ok(content) => CallToolResult::success(vec![ContentBlock::text(content.to_string())]),
            Err(error) => tool_error_result(&error),
        };
        Ok(result.into())
    }
}

pub async fn serve_stdio_with_rmcp(bridge: Arc<McpBridge>) -> Result<(), String> {
    let running = BridgeHandler::new(bridge)
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|error| error.to_string())?;
    running.waiting().await.map_err(|error| error.to_string()).map(|_| ())
}

#[cfg(test)]
#[path = "rmcp_server_tests.rs"]
mod tests;
