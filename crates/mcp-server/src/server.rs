use std::sync::Arc;

use anyhow::Result;
use axum::body::Bytes;
use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

use data_core::identity::NodeIdentity;
use data_core::types::AccessMode;
use data_p2p::dht::DhtIndex;
use data_p2p::torrent::TorrentEngine;
use data_storage::feedback_store::FeedbackStore;
use data_storage::metadata_store::MetadataStore;
use data_trading::escrow::EscrowClient;
use data_trading::mpp::MppClient;
use data_trading::router::PaymentRouter;
use data_trading::x402::X402Client;
use data_search::adapters::default_adapters;
use data_search::engine::SearchEngine;
use data_search::intent::IntentParser;
use data_search::vector_index::VectorIndex;
use data_valuation::tcv::TcvEngine;

use crate::protocol::{McpRequest, McpResponse};
use crate::tools::all_tool_definitions;
use crate::web_ui::INDEX_HTML;

/// Shared state accessible by MCP tool handlers.
pub struct AppState {
    pub identity: NodeIdentity,
    pub dht: DhtIndex,
    pub store: MetadataStore,
    pub feedback_store: FeedbackStore,
    pub tcv_engine: TcvEngine,
    pub search_engine: SearchEngine,
    pub payment_router: PaymentRouter,
    pub torrent_engine: Option<TorrentEngine>,
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
            torrent_engine: None,
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

/// HTTP server: MCP JSON-RPC + REST API + embedded Web UI.
pub async fn run_http(state: Arc<AppState>, port: u16) -> Result<()> {
    let app = Router::new()
        // Web UI
        .route("/", get(serve_ui))
        // REST API for Web UI
        .route("/api/datasets", get(api_list_datasets))
        .route("/api/publish", post(api_publish))
        // MCP JSON-RPC
        .route("/rpc", post(http_rpc_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    info!("Guixu Web UI → http://localhost:{port}");
    info!("MCP HTTP RPC → http://localhost:{port}/rpc");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// --- Web UI handler ---

async fn serve_ui() -> Html<&'static str> {
    Html(INDEX_HTML)
}

// --- REST API: list datasets ---

async fn api_list_datasets(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    info!("api.list_datasets");
    match state.store.list_all() {
        Ok(datasets) => {
            let items: Vec<serde_json::Value> = datasets
                .iter()
                .map(|m| {
                    json!({
                        "cid": m.cid.0,
                        "title": m.title,
                        "description": m.description,
                        "schema": {
                            "columns": m.schema.columns.iter().map(|c| json!({
                                "name": c.name,
                                "dtype": c.dtype,
                            })).collect::<Vec<_>>(),
                            "row_count": m.schema.row_count,
                            "size_bytes": m.schema.size_bytes,
                        },
                        "price": { "amount": m.price.amount, "currency": m.price.currency },
                        "provider": m.provider.0,
                        "access": m.access,
                        "tags": m.tags,
                        "created_at": m.created_at.to_rfc3339(),
                        "updated_at": m.updated_at.to_rfc3339(),
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::Value::Array(items))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

// --- REST API: publish via multipart upload ---

async fn api_publish(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut file_data: Option<(String, Bytes)> = None;
    let mut access = "open".to_string();
    let mut price = 0.0_f64;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let filename = field.file_name().unwrap_or("upload.csv").to_string();
                match field.bytes().await {
                    Ok(bytes) => file_data = Some((filename, bytes)),
                    Err(e) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(json!({"error": format!("Failed to read file: {e}")})),
                        )
                            .into_response()
                    }
                }
            }
            "access" => {
                if let Ok(text) = field.text().await {
                    access = text;
                }
            }
            "price" => {
                if let Ok(text) = field.text().await {
                    price = text.parse().unwrap_or(0.0);
                }
            }
            _ => {
                // consume other fields (privacy_level, epsilon — used by future versions)
                let _ = field.bytes().await;
            }
        }
    }

    let (filename, bytes) = match file_data {
        Some(d) => d,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "No file provided"})),
            )
                .into_response()
        }
    };

    // Write to temp file, then publish
    let tmp_dir = std::env::temp_dir().join("guixu-uploads");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let tmp_path = tmp_dir.join(&filename);

    if let Err(e) = std::fs::write(&tmp_path, &bytes) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to write temp file: {e}")})),
        )
            .into_response();
    }

    let access_mode = if access == "paid" {
        AccessMode::Paid
    } else {
        AccessMode::Open
    };

    match data_p2p::publish::publish_file(
        &tmp_path,
        &state.identity,
        &state.dht,
        &state.store,
        access_mode,
        price,
    )
    .await
    {
        Ok(metadata) => {
            info!(cid = %metadata.cid.0, title = %metadata.title, "api.publish.ok");
            let _ = std::fs::remove_file(&tmp_path);
            (
                StatusCode::OK,
                Json(json!({
                    "cid": metadata.cid.0,
                    "title": metadata.title,
                    "rows": metadata.schema.row_count,
                    "size_bytes": metadata.schema.size_bytes,
                    "columns": metadata.schema.columns.len(),
                })),
            )
                .into_response()
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_path);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

// --- MCP JSON-RPC ---

async fn http_rpc_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpRequest>,
) -> Json<McpResponse> {
    Json(handle_request(req, &state).await)
}

async fn handle_request(req: McpRequest, state: &AppState) -> McpResponse {
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
        _ => Ok(format!("Tool '{name}' not yet implemented")),
    };

    let elapsed_ms = start.elapsed().as_millis();
    match &result {
        Ok(_) => info!(tool = name, elapsed_ms, "mcp.tool.ok"),
        Err(e) => warn!(tool = name, elapsed_ms, error = %e, "mcp.tool.error"),
    }

    result
}
