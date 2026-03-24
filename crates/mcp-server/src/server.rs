use std::sync::Arc;

use anyhow::Result;
use axum::{extract::State, routing::post, Json, Router};
use serde_json::json;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tower_http::cors::CorsLayer;
use tracing::info;

use data_core::identity::NodeIdentity;
use data_p2p::dht::DhtIndex;
use data_storage::feedback_store::FeedbackStore;
use data_storage::metadata_store::MetadataStore;
use data_trading::router::PaymentRouter;
use data_trading::x402::X402Client;
use data_trading::mpp::MppClient;
use data_trading::escrow::EscrowClient;
use data_search::adapters::default_adapters;
use data_search::engine::SearchEngine;
use data_search::intent::IntentParser;
use data_search::vector_index::VectorIndex;
use data_valuation::tcv::TcvEngine;

use crate::protocol::{McpRequest, McpResponse};
use crate::tools::all_tool_definitions;

/// Shared state accessible by MCP tool handlers.
pub struct AppState {
    pub identity: NodeIdentity,
    pub dht: DhtIndex,
    pub store: MetadataStore,
    pub feedback_store: FeedbackStore,
    pub tcv_engine: TcvEngine,
    pub search_engine: SearchEngine,
    pub payment_router: PaymentRouter,
}

impl AppState {
    pub fn new(
        identity: NodeIdentity,
        dht: DhtIndex,
        store: MetadataStore,
        feedback_store: FeedbackStore,
    ) -> Self {
        let vector_index = VectorIndex;
        let intent_parser = IntentParser;
        let adapters = default_adapters();
        let search_engine = SearchEngine::new(vector_index, intent_parser, adapters);

        Self {
            identity,
            dht,
            store,
            feedback_store,
            tcv_engine: TcvEngine,
            search_engine,
            payment_router: PaymentRouter::new(X402Client, MppClient {}, EscrowClient),
        }
    }
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
            Err(e) => McpResponse::error(
                serde_json::Value::Null,
                -32700,
                format!("Parse error: {e}"),
            ),
        };

        let mut out = serde_json::to_vec(&response)?;
        out.push(b'\n');
        stdout.write_all(&out).await?;
        stdout.flush().await?;
    }

    Ok(())
}

/// HTTP bridge for browser-based demo UI.
/// POST /rpc with JSON-RPC body, returns JSON-RPC response.
pub async fn run_http(state: Arc<AppState>, port: u16) -> Result<()> {
    let app = Router::new()
        .route("/rpc", post(http_rpc_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    info!("MCP HTTP bridge listening on http://localhost:{port}/rpc");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn http_rpc_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpRequest>,
) -> Json<McpResponse> {
    Json(handle_request(req, &state).await)
}

async fn handle_request(req: McpRequest, state: &AppState) -> McpResponse {
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
    match name {
        "dataset_search" => crate::handlers::search::handle(args, state).await,
        "dataset_evaluate" => crate::handlers::evaluate::handle(args, state).await,
        "dataset_feedback" => crate::handlers::feedback::handle(args, state).await,
        "dataset_purchase" => crate::handlers::purchase::handle(args, state).await,
        "dataset_reviews" => crate::handlers::reviews::handle(args, state).await,
        "dataset_verify" => crate::handlers::misc::handle_verify(args, state).await,
        "dataset_publish" => crate::handlers::misc::handle_publish(args, state).await,
        _ => Ok(format!("Tool '{name}' not yet implemented")),
    }
}
