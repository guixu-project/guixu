// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use serde_json::json;
use tracing::{info, warn};

use crate::protocol::{
    is_supported_protocol_version, CallToolResult, Implementation, InitializeParams,
    InitializeResult, ListChanged, McpRequest, McpResponse, ServerCapabilities, TextContent,
    INITIALIZED_NOTIFICATION_METHOD, INITIALIZE_METHOD, LATEST_PROTOCOL_VERSION,
};
use crate::server::McpServer;
use crate::tools::validate_tool_definitions;

/// Handle a single MCP JSON-RPC request within the given logical session.
pub async fn handle_request(
    req: McpRequest,
    server: &McpServer,
    session_id: &str,
) -> Option<McpResponse> {
    let request_id = req.id.clone();
    info!(
        method = %req.method,
        id = ?request_id,
        session_id,
        "mcp.request"
    );
    server.sessions().touch(session_id).await;

    match req.method.as_str() {
        INITIALIZE_METHOD => Some(initialize_response(request_id, req.params, server)),

        "ping" => Some(McpResponse::success(
            request_id.unwrap_or(serde_json::Value::Null),
            json!({}),
        )),

        INITIALIZED_NOTIFICATION_METHOD => None,

        "tools/list" => {
            let tools = server.registry().list_definitions();
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

            if tool_name == "dataset_search" && server.state().search_workers > 0 {
                if let Some(summary) = server
                    .sessions()
                    .duplicate_dataset_search_summary(session_id)
                    .await
                {
                    let raw = summary.to_string();
                    warn!(
                        tool = tool_name,
                        session_id, "mcp.tool.blocked_duplicate_search"
                    );
                    return request_id
                        .map(|id| McpResponse::success(id, serialize_tool_result(&raw, true)));
                }
            }

            let Some(tool) = server.registry().get(tool_name) else {
                return request_id.map(|id| {
                    McpResponse::error(id, -32602, format!("unknown tool: {tool_name}"))
                });
            };

            let start = std::time::Instant::now();
            info!(
                tool = tool_name,
                args = %args,
                session_id,
                "mcp.tool.call"
            );

            let result = tool.execute(args.clone(), server.state()).await;
            let elapsed_ms = start.elapsed().as_millis();

            match &result {
                Ok(raw) => {
                    info!(tool = tool_name, elapsed_ms, session_id, "mcp.tool.ok");
                    server
                        .sessions()
                        .record_tool_call(session_id, tool_name, &args, raw, false)
                        .await;
                    request_id.map(|id| McpResponse::success(id, serialize_tool_result(raw, false)))
                }
                Err(error) => {
                    warn!(
                        tool = tool_name,
                        elapsed_ms,
                        session_id,
                        error = %error,
                        "mcp.tool.error"
                    );
                    let raw = error.to_string();
                    server
                        .sessions()
                        .record_tool_call(session_id, tool_name, &args, &raw, true)
                        .await;
                    request_id.map(|id| McpResponse::success(id, serialize_tool_result(&raw, true)))
                }
            }
        }

        _ => request_id
            .map(|id| McpResponse::error(id, -32601, format!("Unknown method: {}", req.method))),
    }
}

fn initialize_response(
    request_id: Option<serde_json::Value>,
    params: Option<serde_json::Value>,
    server: &McpServer,
) -> McpResponse {
    let id = request_id.unwrap_or(serde_json::Value::Null);
    let tools = server.registry().list_definitions();

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

    let supports_sampling = initialize_params.capabilities.sampling.is_some();
    if let Some(runtime) = server.state().sampling_runtime.as_ref() {
        runtime.set_supports_sampling(supports_sampling);
    }

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
