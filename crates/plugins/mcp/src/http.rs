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
use data_search::adapters::load_open_data_skills;

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
        .route("/api/node/status", get(api_node_status))
        .route("/api/datasets", get(api_list_datasets))
        .route("/api/datasets/{cid}", get(api_dataset_detail))
        .route("/api/datasets/{cid}/preview", get(api_dataset_preview))
        .route("/api/datasets/{cid}/stats", get(api_dataset_stats))
        .route("/api/publish", post(api_publish))
        .route("/api/unpublish/{cid}", axum::routing::delete(api_unpublish))
        .route("/api/network/peers", get(api_network_peers))
        .route("/api/network/nat", get(api_network_nat))
        .route("/api/market/search", get(api_market_search))
        .route("/api/market/{cid}/preview", get(api_market_preview))
        .route("/api/wallet/balance", get(api_wallet_balance))
        .route("/api/wallet/transactions", get(api_wallet_transactions))
        .route("/api/traces", get(api_list_traces))
        .route("/api/traces/{trace_id}/spans", get(api_trace_spans))
        .route("/api/traces/{trace_id}/scores", get(api_trace_scores))
        .route("/api/memory/timeline", get(api_memory_timeline))
        .route("/api/skills", get(api_list_skills))
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

async fn api_node_status(State(server): State<Arc<McpServer>>) -> impl IntoResponse {
    let seeds = server.state().store.list_seeds().unwrap_or_default();
    let uptime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Json(json!({
        "status": "running",
        "did": server.state().identity.did.0,
        "peer_id": format!("{}", server.state().dht.handle().local_peer_id),
        "seeding_count": seeds.len(),
        "uptime": uptime,
        "version": env!("CARGO_PKG_VERSION"),
    }))
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

async fn api_dataset_detail(
    State(server): State<Arc<McpServer>>,
    axum::extract::Path(cid): axum::extract::Path<String>,
) -> impl IntoResponse {
    let dataset_cid = data_core::types::DatasetCid(cid);
    match server.state().store.get(&dataset_cid) {
        Ok(Some(m)) => Json(json!({
            "cid": m.cid.0,
            "title": m.title,
            "description": m.description,
            "schema": { "columns": m.schema.columns, "row_count": m.schema.row_count, "size_bytes": m.schema.size_bytes },
            "price": { "amount": m.price.amount, "currency": m.price.currency },
            "provider": m.provider.0,
            "access": m.access,
            "tags": m.tags,
            "info_hash": m.info_hash,
        }))
        .into_response(),
        _ => (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response(),
    }
}

async fn api_dataset_preview(
    State(server): State<Arc<McpServer>>,
    axum::extract::Path(cid): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let rows: usize = params
        .get("rows")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let dataset_cid = data_core::types::DatasetCid(cid);
    let file_path = match server.state().store.get_file_path(&dataset_cid) {
        Ok(Some(p)) if p.exists() => p,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "file not found"})),
            )
                .into_response()
        }
    };
    match std::fs::read(&file_path) {
        Ok(content) => {
            let text = String::from_utf8_lossy(&content);
            let lines: Vec<&str> = text.lines().take(rows + 1).collect();
            Json(json!({"rows": lines})).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn api_dataset_stats(
    State(server): State<Arc<McpServer>>,
    axum::extract::Path(cid): axum::extract::Path<String>,
) -> impl IntoResponse {
    let dataset_cid = data_core::types::DatasetCid(cid.clone());
    match server.state().store.get(&dataset_cid) {
        Ok(Some(m)) => Json(json!({
            "cid": cid,
            "row_count": m.schema.row_count,
            "size_bytes": m.schema.size_bytes,
            "columns": m.schema.columns.len(),
        }))
        .into_response(),
        _ => (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response(),
    }
}

async fn api_unpublish(
    State(server): State<Arc<McpServer>>,
    axum::extract::Path(cid): axum::extract::Path<String>,
) -> impl IntoResponse {
    let dataset_cid = data_core::types::DatasetCid(cid.clone());
    match server.state().store.get(&dataset_cid) {
        Ok(Some(m)) => {
            if let Some(ref info_hash) = m.info_hash {
                let _ = server.state().store.delete_seed(info_hash);
            }
            let _ = server.state().store.mark_unpublished(&dataset_cid);
            Json(json!({"status": "unpublished", "cid": cid})).into_response()
        }
        _ => (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response(),
    }
}

async fn api_network_peers(State(server): State<Arc<McpServer>>) -> impl IntoResponse {
    Json(json!({
        "local_peer_id": format!("{}", server.state().dht.handle().local_peer_id),
        "peers": [],
    }))
}

async fn api_network_nat(State(_server): State<Arc<McpServer>>) -> impl IntoResponse {
    Json(json!({
        "nat_type": "unknown",
        "relay_enabled": true,
    }))
}

async fn api_market_search(
    State(server): State<Arc<McpServer>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let query = params.get("q").cloned().unwrap_or_default();
    if query.is_empty() {
        return Json(json!({"results": []})).into_response();
    }
    let local_metadata = server.state().store.list_all().unwrap_or_default();
    let signal_fetcher: data_search::engine::SignalFetcher =
        Box::new(|_cid: &str| data_core::feedback::CommunitySignal {
            dataset_cid: data_core::types::DatasetCid(String::new()),
            total_reviews: 0,
            avg_relevance: 0.0,
            avg_quality: 0.0,
            positive_rate: 0.0,
            negative_rate: 0.0,
            task_signals: vec![],
        });
    match server
        .state()
        .search_engine
        .search(
            &query,
            &Default::default(),
            &local_metadata,
            &signal_fetcher,
            20,
        )
        .await
    {
        Ok(output) => {
            let items: Vec<serde_json::Value> = output
                .results
                .iter()
                .map(|r| {
                    json!({
                        "cid": r.result.cid.0,
                        "title": r.result.title,
                        "description": r.result.description,
                        "source": r.result.source,
                        "price": { "amount": r.result.price.amount },
                    })
                })
                .collect();
            Json(json!({"results": items})).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn api_market_preview(
    State(server): State<Arc<McpServer>>,
    axum::extract::Path(cid): axum::extract::Path<String>,
) -> impl IntoResponse {
    // Try local store first, then would use P2P sample protocol
    let dataset_cid = data_core::types::DatasetCid(cid);
    match server.state().store.get(&dataset_cid) {
        Ok(Some(m)) => Json(json!({
            "cid": m.cid.0,
            "schema": m.schema,
            "source": "local",
        }))
        .into_response(),
        _ => (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response(),
    }
}

async fn api_wallet_balance(State(_server): State<Arc<McpServer>>) -> impl IntoResponse {
    Json(json!({
        "balance": "0.00",
        "currency": "USDC",
        "network": "base-sepolia",
    }))
}

async fn api_wallet_transactions(State(_server): State<Arc<McpServer>>) -> impl IntoResponse {
    Json(json!({"transactions": []}))
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
    State(server): State<Arc<McpServer>>,
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

    let result = if let Some(pool) = server.trace_pool() {
        match pool.get().await {
            Ok(store) => {
                let pool_tx = pool.sender();
                tokio::task::spawn_blocking(move || {
                    let res = store.list_traces(&source, limit);
                    let _ = pool_tx.try_send(store);
                    res
                })
                .await
            }
            Err(e) => Ok(Err(e)),
        }
    } else {
        let db_path = trace_db_path();
        tokio::task::spawn_blocking(move || {
            let store =
                data_storage::trace_store::TraceStore::open(std::path::Path::new(&db_path))?;
            store.list_traces(&source, limit)
        })
        .await
    };

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
    State(server): State<Arc<McpServer>>,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let source = params
        .get("source")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "guixu".into());

    let result = if let Some(pool) = server.trace_pool() {
        match pool.get().await {
            Ok(store) => {
                let pool_tx = pool.sender();
                tokio::task::spawn_blocking(move || {
                    let res = store.get_trace_spans(&trace_id, &source);
                    let _ = pool_tx.try_send(store);
                    res
                })
                .await
            }
            Err(e) => Ok(Err(e)),
        }
    } else {
        let db_path = trace_db_path();
        tokio::task::spawn_blocking(move || {
            let store =
                data_storage::trace_store::TraceStore::open(std::path::Path::new(&db_path))?;
            store.get_trace_spans(&trace_id, &source)
        })
        .await
    };

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
    State(server): State<Arc<McpServer>>,
    axum::extract::Path(trace_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let result = if let Some(pool) = server.trace_pool() {
        match pool.get().await {
            Ok(store) => {
                let pool_tx = pool.sender();
                tokio::task::spawn_blocking(move || {
                    let res = store.get_scores_for_trace(&trace_id);
                    let _ = pool_tx.try_send(store);
                    res
                })
                .await
            }
            Err(e) => Ok(Err(e)),
        }
    } else {
        let db_path = trace_db_path();
        tokio::task::spawn_blocking(move || {
            let store =
                data_storage::trace_store::TraceStore::open(std::path::Path::new(&db_path))?;
            store.get_scores_for_trace(&trace_id)
        })
        .await
    };

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
    State(server): State<Arc<McpServer>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let memory_key = params.get("memory_key").cloned().unwrap_or_default();
    let limit: i64 = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);

    if memory_key.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "memory_key is required"})),
        )
            .into_response();
    }

    let result = if let Some(pool) = server.trace_pool() {
        match pool.get().await {
            Ok(store) => {
                let pool_tx = pool.sender();
                tokio::task::spawn_blocking(move || {
                    let res = store.memory_timeline(&memory_key, None, limit);
                    let _ = pool_tx.try_send(store);
                    res
                })
                .await
            }
            Err(e) => Ok(Err(e)),
        }
    } else {
        let db_path = trace_db_path();
        tokio::task::spawn_blocking(move || {
            let store =
                data_storage::trace_store::TraceStore::open(std::path::Path::new(&db_path))?;
            store.memory_timeline(&memory_key, None, limit)
        })
        .await
    };

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

async fn api_list_skills() -> impl IntoResponse {
    let skills = load_open_data_skills().unwrap_or_default();
    let items: Vec<serde_json::Value> = skills
        .into_iter()
        .map(|s| {
            let provider_kind = match &s.provider {
                data_search::adapters::SkillProvider::NativeAdapter { adapter } => {
                    serde_json::json!({ "kind": "native_adapter", "adapter": adapter })
                }
                data_search::adapters::SkillProvider::HttpSearch {
                    base_url,
                    operations,
                    ..
                } => {
                    serde_json::json!({
                        "kind": "http_search",
                        "base_url": base_url,
                        "operations": {
                            "search": {
                                "path": operations.search.path,
                                "method": operations.search.method,
                                "query_param": operations.search.query_param,
                                "limit_param": operations.search.limit_param.clone(),
                            }
                        },
                    })
                }
                data_search::adapters::SkillProvider::SqlCatalog {
                    engine,
                    catalogs,
                    nl2sql: _,
                } => {
                    serde_json::json!({
                        "kind": "sql",
                        "engine": format!("{:?}", engine),
                        "catalogs": format!("{:?}", catalogs),
                    })
                }
                data_search::adapters::SkillProvider::WsJsonRpc {
                    base_url,
                    subscriptions,
                    ..
                } => {
                    serde_json::json!({
                        "kind": "ws_json_rpc",
                        "base_url": base_url,
                        "subscriptions": format!("{:?}", subscriptions),
                    })
                }
                data_search::adapters::SkillProvider::GrpcStream {
                    endpoint,
                    service,
                    rpc_name,
                    ..
                } => {
                    serde_json::json!({
                        "kind": "grpc_stream",
                        "endpoint": endpoint,
                        "service": service,
                        "rpc_name": rpc_name,
                    })
                }
                data_search::adapters::SkillProvider::Subgraph {
                    subgraph_url,
                    query,
                    poll_interval_ms,
                    ..
                } => {
                    serde_json::json!({
                        "kind": "subgraph",
                        "subgraph_url": subgraph_url,
                        "query": query,
                        "poll_interval_ms": poll_interval_ms,
                    })
                }
            };
            let sample = s.sample.as_ref().map(|sp| {
                serde_json::json!({
                    "endpoint": sp.endpoint,
                    "id_param": sp.id_param,
                    "range_header": sp.range_header,
                    "max_bytes": sp.max_bytes,
                    "auth": {
                        "kind": format!("{:?}", sp.auth),
                    },
                    "parse_mode": format!("{:?}", sp.parse_mode),
                })
            });
            serde_json::json!({
                "spec_version": s.spec_version,
                "id": s.id,
                "name": s.name,
                "description": s.description,
                "source": s.source,
                "tags": s.tags,
                "routing_hints": s.routing_hints,
                "enabled": s.enabled,
                "capabilities": {
                    "search": s.capabilities.search,
                    "lookup": s.capabilities.lookup,
                    "download": s.capabilities.download,
                    "schema_probe": s.capabilities.schema_probe,
                    "sample_preview": s.capabilities.sample_preview,
                    "license_lookup": s.capabilities.license_lookup,
                },
                "governance": {
                    "trust_tier": format!("{:?}", s.governance.trust_tier).to_lowercase(),
                    "provenance_hint": s.governance.provenance_hint,
                    "compliance_hint": s.governance.compliance_hint,
                },
                "provider": provider_kind,
                "sample": sample,
            })
        })
        .collect();
    Json(serde_json::json!({ "skills": items })).into_response()
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
