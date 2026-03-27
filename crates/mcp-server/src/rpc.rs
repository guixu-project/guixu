use anyhow::Result;
use serde_json::json;
use tracing::{info, warn};

use crate::protocol::{McpRequest, McpResponse};
use crate::state::AppState;
use crate::tools::all_tool_definitions;

/// Handle a single MCP JSON-RPC request.
pub async fn handle_request(req: McpRequest, state: &AppState) -> McpResponse {
    info!(method = %req.method, id = %req.id, "mcp.request");

    match req.method.as_str() {
        "initialize" => McpResponse::success(
            req.id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "guixu", "version": "0.1.0" }
            }),
        ),

        "tools/list" => {
            let tools = all_tool_definitions();
            McpResponse::success(req.id, json!({ "tools": tools }))
        }

        "tools/call" => {
            let params = req.params.unwrap_or_default();
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_default();

            match dispatch_tool(tool_name, args, state).await {
                Ok(result) => McpResponse::success(
                    req.id,
                    json!({ "content": [{ "type": "text", "text": result }] }),
                ),
                Err(e) => McpResponse::error(req.id, -32000, e.to_string()),
            }
        }

        _ => McpResponse::error(req.id, -32601, format!("Unknown method: {}", req.method)),
    }
}

async fn dispatch_tool(name: &str, args: serde_json::Value, state: &AppState) -> Result<String> {
    let start = std::time::Instant::now();
    info!(tool = name, args = %args, "mcp.tool.call");

    let result = match name {
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
        _ => Ok(format!("Tool '{name}' not yet implemented")),
    };

    let elapsed_ms = start.elapsed().as_millis();
    match &result {
        Ok(_) => info!(tool = name, elapsed_ms, "mcp.tool.ok"),
        Err(e) => warn!(tool = name, elapsed_ms, error = %e, "mcp.tool.error"),
    }

    result
}
