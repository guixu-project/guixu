use std::sync::Arc;

use anyhow::Result;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::info;

use crate::protocol::{McpRequest, McpResponse};
use crate::rpc::handle_request;
use crate::state::AppState;

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
            Err(e) => {
                McpResponse::error(serde_json::Value::Null, -32700, format!("Parse error: {e}"))
            }
        };

        let mut out = serde_json::to_vec(&response)?;
        out.push(b'\n');
        stdout.write_all(&out).await?;
        stdout.flush().await?;
    }

    Ok(())
}
