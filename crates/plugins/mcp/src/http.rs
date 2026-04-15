// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use axum::body::Bytes;
use axum::extract::{Multipart, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use tower_http::cors::CorsLayer;
use tracing::info;

use data_auth::privacy::{PrivacyConfig, PrivacyLevel};
use data_core::types::AccessMode;

use crate::demo_ui;
use crate::prism_ui;
use crate::protocol::McpRequest;
use crate::rpc::handle_request;
use crate::server::McpServer;
use crate::web_ui::INDEX_HTML;

/// HTTP server: MCP JSON-RPC + REST API + embedded Web UI.
pub async fn run_http(server: Arc<McpServer>, port: u16) -> Result<()> {
    let app = Router::new()
        .route("/", get(serve_ui))
        .route("/demo", get(demo_ui::serve_demo))
        .route("/demo/style.css", get(demo_ui::serve_demo_css))
        .route("/demo/{file}", get(demo_ui::serve_demo_js))
        .route("/trace", get(demo_ui::serve_trace))
        .route("/trace/{file}", get(demo_ui::serve_trace_asset))
        .route("/prism", get(prism_ui::serve_prism))
        .route("/prism/", get(prism_ui::serve_prism))
        .route("/prism/assets/{file}", get(prism_ui::serve_prism_asset))
        .route("/api/datasets", get(api_list_datasets))
        .route("/api/publish", post(api_publish))
        .route("/api/traces", get(api_list_traces))
        .route("/api/traces/{trace_id}/spans", get(api_trace_spans))
        .route("/api/traces/{trace_id}/scores", get(api_trace_scores))
        .route("/api/memory/timeline", get(api_memory_timeline))
        .route("/mcp", post(http_rpc_handler))
        .route("/rpc", post(http_rpc_handler))
        .layer(CorsLayer::permissive())
        .with_state(server);

    let addr = format!("0.0.0.0:{port}");
    info!("Guixu Web UI → http://localhost:{port}");
    info!("Guixu Demo UI → http://localhost:{port}/demo");
    info!("Guixu Trace UI → http://localhost:{port}/trace");
    info!("Guixu Prism UI → http://localhost:{port}/prism");
    info!("MCP HTTP RPC → http://localhost:{port}/mcp");
    info!("Legacy MCP RPC alias → http://localhost:{port}/rpc");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn serve_ui() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn api_list_datasets(State(server): State<Arc<McpServer>>) -> impl IntoResponse {
    info!("api.list_datasets");
    match server.state().store.list_all() {
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

async fn api_publish(
    State(server): State<Arc<McpServer>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut file_data: Option<(String, Bytes)> = None;
    let mut access = "open".to_string();
    let mut price = 0.0_f64;
    let mut privacy_level = "standard".to_string();
    let mut epsilon = "1.0".to_string();

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
            "privacy_level" => {
                if let Ok(text) = field.text().await {
                    privacy_level = text;
                }
            }
            "epsilon" => {
                if let Ok(text) = field.text().await {
                    epsilon = text;
                }
            }
            _ => {
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

    let privacy = match parse_publish_privacy(&privacy_level, &epsilon) {
        Ok(config) => config,
        Err(message) => {
            let _ = std::fs::remove_file(&tmp_path);
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response();
        }
    };

    match data_p2p::publish::publish_file_with_privacy(
        &tmp_path,
        &server.state().identity,
        &server.state().dht,
        &server.state().store,
        access_mode,
        price,
        &privacy,
        false,
    )
    .await
    {
        Ok(metadata) => {
            info!(
                cid = %metadata.cid.0,
                title = %metadata.title,
                privacy = ?privacy.level,
                epsilon = privacy.epsilon,
                "api.publish.ok"
            );
            let _ = std::fs::remove_file(&tmp_path);
            (
                StatusCode::OK,
                Json(json!({
                    "cid": metadata.cid.0,
                    "title": metadata.title,
                    "rows": metadata.schema.row_count,
                    "size_bytes": metadata.schema.size_bytes,
                    "columns": metadata.schema.columns.len(),
                    "privacy_level": format!("{:?}", privacy.level).to_lowercase(),
                    "epsilon": privacy.epsilon,
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

pub(crate) fn parse_publish_privacy(
    level: &str,
    epsilon: &str,
) -> std::result::Result<PrivacyConfig, String> {
    let level = match level.trim().to_lowercase().as_str() {
        "" | "standard" => PrivacyLevel::Standard,
        "off" => PrivacyLevel::Off,
        "strict" => PrivacyLevel::Strict,
        other => {
            return Err(format!(
                "Invalid privacy_level '{other}'. Expected off, standard, or strict."
            ))
        }
    };

    let epsilon = if epsilon.trim().is_empty() {
        1.0
    } else {
        epsilon
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("Invalid epsilon '{epsilon}'. Expected a positive number."))?
    };

    if !epsilon.is_finite() || epsilon <= 0.0 {
        return Err("epsilon must be a positive finite number.".into());
    }

    Ok(PrivacyConfig {
        level,
        epsilon,
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// Trace API endpoints
//
// Each request opens a fresh DuckDB connection via spawn_blocking.
// DuckDB Connection is not Sync (uses RefCell internally), so it cannot
// be stored in Arc<AppState>. Opening a file-backed DuckDB is cheap (~1ms),
// and this avoids all threading hazards documented in AGENTS.md.
// ---------------------------------------------------------------------------

fn trace_db_path() -> String {
    data_core::config::NodeConfig::load_or_default()
        .trace
        .db_path
}

async fn api_list_traces(
    State(_): State<Arc<McpServer>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let source = params
        .get("source")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "guixu".into());
    let limit: i64 = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    let db_path = trace_db_path();

    let result = tokio::task::spawn_blocking(move || {
        let store = data_storage::trace_store::TraceStore::open(std::path::Path::new(&db_path))?;
        store.list_traces(&source, limit)
    })
    .await;

    match result {
        Ok(Ok(traces)) => Json(json!(traces)).into_response(),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn api_trace_spans(
    State(_): State<Arc<McpServer>>,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let source = params
        .get("source")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "guixu".into());
    let db_path = trace_db_path();

    let result = tokio::task::spawn_blocking(move || {
        let store = data_storage::trace_store::TraceStore::open(std::path::Path::new(&db_path))?;
        store.get_trace_spans(&trace_id, &source)
    })
    .await;

    match result {
        Ok(Ok(spans)) => Json(json!(spans)).into_response(),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn api_trace_scores(
    State(_): State<Arc<McpServer>>,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let db_path = trace_db_path();

    let result = tokio::task::spawn_blocking(move || {
        let store = data_storage::trace_store::TraceStore::open(std::path::Path::new(&db_path))?;
        store.get_scores_for_trace(&trace_id)
    })
    .await;

    match result {
        Ok(Ok(scores)) => Json(json!(scores)).into_response(),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn api_memory_timeline(
    State(_): State<Arc<McpServer>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let memory_key = params.get("memory_key").cloned().unwrap_or_default();
    let limit: i64 = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    let db_path = trace_db_path();

    if memory_key.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "memory_key is required"})),
        )
            .into_response();
    }

    let result = tokio::task::spawn_blocking(move || {
        let store = data_storage::trace_store::TraceStore::open(std::path::Path::new(&db_path))?;
        store.memory_timeline(&memory_key, None, limit)
    })
    .await;

    match result {
        Ok(Ok(spans)) => Json(json!(spans)).into_response(),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn http_rpc_handler(
    State(server): State<Arc<McpServer>>,
    headers: HeaderMap,
    Json(req): Json<McpRequest>,
) -> impl IntoResponse {
    let session_id = http_session_id(&headers);

    // Extract trace context from incoming traceparent header and store in manager
    if let Some(ctx) = crate::trace_context::extract_trace_context(&headers) {
        if let Some(tm) = &server.state().trace_manager {
            (*tm).blocking_read().set_current_context_sync(ctx);
        }
    }

    match handle_request(req, &server, &session_id).await {
        Some(r) => Json(r).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

fn http_session_id(headers: &HeaderMap) -> String {
    headers
        .get("mcp-session-id")
        .or_else(|| headers.get("x-guixu-session-id"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "http-default".to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_publish_privacy;
    use data_auth::privacy::PrivacyLevel;

    #[test]
    fn parse_publish_privacy_uses_defaults() {
        let config = parse_publish_privacy("", "").unwrap();
        assert_eq!(config.level, PrivacyLevel::Standard);
        assert_eq!(config.epsilon, 1.0);
    }

    #[test]
    fn parse_publish_privacy_accepts_explicit_values() {
        let config = parse_publish_privacy("strict", "0.5").unwrap();
        assert_eq!(config.level, PrivacyLevel::Strict);
        assert_eq!(config.epsilon, 0.5);
    }

    #[test]
    fn parse_publish_privacy_rejects_invalid_level() {
        let err = parse_publish_privacy("private", "1.0").unwrap_err();
        assert!(err.contains("Invalid privacy_level"));
    }

    #[test]
    fn parse_publish_privacy_rejects_invalid_epsilon() {
        let err = parse_publish_privacy("standard", "0").unwrap_err();
        assert!(err.contains("positive finite"));
    }
}
