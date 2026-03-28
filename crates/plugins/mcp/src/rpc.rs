use anyhow::Result;
use serde_json::json;
use tracing::{info, warn};

use crate::protocol::{
    is_supported_protocol_version, CallToolResult, Implementation, InitializeParams,
    InitializeResult, ListChanged, McpRequest, McpResponse, ServerCapabilities, TextContent,
    INITIALIZED_NOTIFICATION_METHOD, INITIALIZE_METHOD, LATEST_PROTOCOL_VERSION,
};
use crate::state::{AppState, ToolProfile};
use crate::tools::{all_tool_definitions, codex_tool_definitions, validate_tool_definitions};

fn available_tool_definitions(state: &AppState) -> Vec<crate::protocol::ToolDefinition> {
    match state.tool_profile {
        ToolProfile::Full => all_tool_definitions(),
        ToolProfile::CodexWorkflow => codex_tool_definitions(),
    }
}

/// Handle a single MCP JSON-RPC request.
pub async fn handle_request(req: McpRequest, state: &AppState) -> Option<McpResponse> {
    let request_id = req.id.clone();
    info!(method = %req.method, id = ?request_id, "mcp.request");

    match req.method.as_str() {
        INITIALIZE_METHOD => Some(initialize_response(request_id, req.params, state)),

        "ping" => Some(McpResponse::success(
            request_id.unwrap_or(serde_json::Value::Null),
            json!({}),
        )),

        INITIALIZED_NOTIFICATION_METHOD => None,

        "tools/list" => {
            let tools = available_tool_definitions(state);
            request_id.map(|id| McpResponse::success(id, json!({ "tools": tools })))
        }

        "tools/call" => {
            let params = req.params.unwrap_or_default();
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_default();

            if tool_name.trim().is_empty() {
                return request_id
                    .map(|id| McpResponse::error(id, -32602, "missing tool name".to_string()));
            }

            if !available_tool_definitions(state)
                .iter()
                .any(|definition| definition.name == tool_name)
            {
                return request_id.map(|id| {
                    McpResponse::error(id, -32602, format!("unknown tool: {tool_name}"))
                });
            }

            match dispatch_tool(tool_name, args, state).await {
                Ok(result) => request_id
                    .map(|id| McpResponse::success(id, serialize_tool_result(&result, false))),
                Err(e) => request_id.map(|id| {
                    McpResponse::success(id, serialize_tool_result(&e.to_string(), true))
                }),
            }
        }

        _ => request_id
            .map(|id| McpResponse::error(id, -32601, format!("Unknown method: {}", req.method))),
    }
}

fn initialize_response(
    request_id: Option<serde_json::Value>,
    params: Option<serde_json::Value>,
    state: &AppState,
) -> McpResponse {
    let id = request_id.unwrap_or(serde_json::Value::Null);
    let tools = available_tool_definitions(state);

    if let Err(message) = validate_tool_definitions(&tools) {
        return McpResponse::error(id, -32600, message);
    }

    let initialize_params = match params {
        Some(value) => match serde_json::from_value::<InitializeParams>(value) {
            Ok(parsed) => parsed,
            Err(e) => {
                return McpResponse::error(id, -32600, format!("invalid initialize request: {e}"))
            }
        },
        None => InitializeParams::default(),
    };

    let protocol_version = initialize_params
        .protocol_version
        .as_deref()
        .filter(|version| is_supported_protocol_version(version))
        .unwrap_or(LATEST_PROTOCOL_VERSION)
        .to_string();

    let result = InitializeResult {
        protocol_version,
        capabilities: ServerCapabilities {
            tools: Some(ListChanged {
                list_changed: Some(false),
            }),
        },
        server_info: Implementation {
            name: "guixu".to_string(),
            version: "0.1.0".to_string(),
        },
    };

    match serde_json::to_value(result) {
        Ok(value) => McpResponse::success(id, value),
        Err(e) => McpResponse::error(
            id,
            -32603,
            format!("failed to serialize initialize response: {e}"),
        ),
    }
}

fn serialize_tool_result(raw: &str, is_error: bool) -> serde_json::Value {
    let structured_content = match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Object(map)) => Some(map),
        _ => None,
    };
    let result = CallToolResult {
        content: vec![TextContent {
            kind: "text".to_string(),
            text: raw.to_string(),
        }],
        is_error,
        structured_content,
    };
    serde_json::to_value(result).unwrap_or_else(|_| {
        json!({
            "content": [{ "type": "text", "text": raw }],
            "isError": is_error,
        })
    })
}

async fn dispatch_tool(name: &str, args: serde_json::Value, state: &AppState) -> Result<String> {
    let start = std::time::Instant::now();
    info!(tool = name, args = %args, "mcp.tool.call");

    let result = match name {
        "intent_parse" => crate::handlers::intent::handle(args, state).await,
        "task_pipeline" => crate::handlers::task_pipeline::handle(args, state).await,
        "dataset_search" => crate::handlers::search::handle(args, state).await,
        "dataset_evaluate" => crate::handlers::evaluate::handle(args, state).await,
        "dataset_feedback" => crate::handlers::feedback::handle(args, state).await,
        "dataset_purchase" => crate::handlers::purchase::handle(args, state).await,
        "dataset_reviews" => crate::handlers::reviews::handle(args, state).await,
        "dataset_verify" => crate::handlers::misc::handle_verify(args, state).await,
        "dataset_publish" => crate::handlers::misc::handle_publish(args, state).await,
        "dataset_bt_download" => crate::handlers::bt_download::handle(args, state).await,
        "dataset_bt_preview" => crate::handlers::bt_download::handle_preview(args, state).await,
        "dataset_bt_stats" => crate::handlers::bt_download::handle_stats(args, state).await,
        _ => anyhow::bail!("unknown tool: {name}"),
    };

    let elapsed_ms = start.elapsed().as_millis();
    match &result {
        Ok(_) => info!(tool = name, elapsed_ms, "mcp.tool.ok"),
        Err(e) => warn!(tool = name, elapsed_ms, error = %e, "mcp.tool.error"),
    }

    result
}
