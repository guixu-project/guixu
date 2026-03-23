use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{error, info};

use data_core::identity::NodeIdentity;
use data_core::types::{AccessMode, DatasetCid};
use data_p2p::dht::DhtIndex;
use data_p2p::storage::MetadataStore;

use crate::protocol::{McpRequest, McpResponse};
use crate::tools::all_tool_definitions;

/// Shared state accessible by MCP tool handlers.
pub struct AppState {
    pub identity: NodeIdentity,
    pub dht: DhtIndex,
    pub store: MetadataStore,
}

/// MCP Server — reads JSON-RPC from stdin, writes to stdout.
pub async fn run_stdio(state: Arc<AppState>) -> Result<()> {
    let stdin = BufReader::new(io::stdin());
    let mut stdout = io::stdout();
    let mut lines = stdin.lines();

    info!("MCP server started on stdio");

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<McpRequest>(&line) {
            Ok(req) => handle_request(req, &state).await,
            Err(e) => McpResponse::error(serde_json::Value::Null, -32700, format!("Parse error: {e}")),
        };

        let mut out = serde_json::to_vec(&response)?;
        out.push(b'\n');
        stdout.write_all(&out).await?;
        stdout.flush().await?;
    }

    Ok(())
}

async fn handle_request(req: McpRequest, state: &AppState) -> McpResponse {
    match req.method.as_str() {
        "initialize" => McpResponse::success(
            req.id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "dataset-protocol", "version": "0.1.0" }
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
    match name {
        "dataset_search" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            // Search local store for matching metadata
            let all = state.store.list_all()?;
            let query_lower = query.to_lowercase();
            let results: Vec<_> = all
                .iter()
                .filter(|m| {
                    m.title.to_lowercase().contains(&query_lower)
                        || m.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
                        || m.description.as_deref().unwrap_or("").to_lowercase().contains(&query_lower)
                })
                .collect();

            Ok(serde_json::to_string_pretty(&results)?)
        }

        "dataset_verify" => {
            let cid_str = args.get("cid").and_then(|v| v.as_str()).unwrap_or("");
            let cid = DatasetCid(cid_str.to_string());
            match state.store.get(&cid)? {
                Some(metadata) => {
                    let report = data_auth::verifier::verify(&metadata, None)?;
                    Ok(format!("{report:?}"))
                }
                None => Ok(format!("Dataset {cid_str} not found in local store")),
            }
        }

        "dataset_publish" => {
            let file_path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            let path = std::path::Path::new(file_path);
            if !path.exists() {
                anyhow::bail!("File not found: {file_path}");
            }
            let metadata = data_p2p::publish::publish_file(
                path,
                &state.identity,
                &state.dht,
                &state.store,
                AccessMode::Open,
                0.0,
            )
            .await?;
            Ok(serde_json::to_string_pretty(&metadata)?)
        }

        _ => Ok(format!("Tool '{name}' not yet implemented")),
    }
}
