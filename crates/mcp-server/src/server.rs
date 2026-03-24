use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::info;

use data_core::feedback::{CommunitySignal, DatasetFeedback, ValueAssessment};
use data_core::identity::NodeIdentity;
use data_core::types::{AccessMode, DatasetCid};
use data_p2p::dht::DhtIndex;
use data_p2p::feedback_store::FeedbackStore;
use data_p2p::storage::MetadataStore;
use data_search::adapters::default_adapters;
use data_search::engine::{SearchEngine, SearchFilters};
use data_search::intent::IntentParser;
use data_search::vector_index::VectorIndex;
use data_valuation::tcv::{TaskContext, TcvEngine};

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
        // ---------------------------------------------------------------
        // dataset_search: multi-source search with TCV-based ranking
        // ---------------------------------------------------------------
        "dataset_search" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

            let filter_obj = args.get("filters").cloned().unwrap_or_default();
            let filters = SearchFilters {
                topic: filter_obj.get("topic").and_then(|v| v.as_str()).map(String::from),
                min_rows: filter_obj.get("min_rows").and_then(|v| v.as_u64()),
                max_price: filter_obj.get("max_price").and_then(|v| v.as_f64()),
                license: filter_obj.get("license").and_then(|v| v.as_str()).map(String::from),
                min_quality: filter_obj.get("min_quality").and_then(|v| v.as_f64()),
                source: filter_obj.get("source").and_then(|v| v.as_str()).map(String::from),
            };

            // Load all local metadata for P2P search
            let local_metadata = state.store.list_all()?;

            // Build signal fetcher closure over feedback store
            let fb_store = state.feedback_store.clone();
            let signal_fetcher: data_search::engine::SignalFetcher =
                Box::new(move |cid_str: &str| {
                    let cid = DatasetCid(cid_str.to_string());
                    fb_store.compute_signal(&cid).unwrap_or_else(|_| CommunitySignal {
                        dataset_cid: cid,
                        total_reviews: 0,
                        avg_relevance: 0.0,
                        avg_quality: 0.0,
                        positive_rate: 0.0,
                        negative_rate: 0.0,
                        task_signals: vec![],
                    })
                });

            let ranked = state
                .search_engine
                .search(query, &filters, &local_metadata, &signal_fetcher, limit)
                .await?;

            // Format output with ranking info
            let output: Vec<serde_json::Value> = ranked
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    json!({
                        "rank": i + 1,
                        "cid": r.result.cid.0,
                        "title": r.result.title,
                        "description": r.result.description,
                        "source": r.result.source,
                        "price": r.result.price,
                        "schema": {
                            "columns": r.result.schema.columns.len(),
                            "rows": r.result.schema.row_count,
                            "size_bytes": r.result.schema.size_bytes,
                        },
                        "rank_score": format!("{:.1}", r.rank_score),
                        "community": {
                            "total_reviews": r.signal.total_reviews,
                            "avg_relevance": format!("{:.2}", r.signal.avg_relevance),
                            "positive_rate": format!("{:.0}%", r.signal.positive_rate * 100.0),
                            "negative_rate": format!("{:.0}%", r.signal.negative_rate * 100.0),
                        }
                    })
                })
                .collect();

            Ok(serde_json::to_string_pretty(&output)?)
        }

        // ---------------------------------------------------------------
        // dataset_evaluate: TCV (Task-Conditioned Value) computation
        // ---------------------------------------------------------------
        "dataset_evaluate" => {
            let cid_str = args.get("cid").and_then(|v| v.as_str()).unwrap_or("");
            let cid = DatasetCid(cid_str.to_string());

            let metadata = match state.store.get(&cid)? {
                Some(m) => m,
                None => anyhow::bail!("Dataset {cid_str} not found"),
            };

            let task_desc = args
                .get("task_description")
                .and_then(|v| v.as_str())
                .unwrap_or("general analysis");
            let task_type = args
                .get("task_type")
                .and_then(|v| v.as_str())
                .unwrap_or("general");
            let required_cols: Vec<String> = args
                .get("required_columns")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let budget = args.get("budget").and_then(|v| v.as_f64()).unwrap_or(0.0);

            let task = TaskContext {
                task_description: task_desc.to_string(),
                task_type: task_type.to_string(),
                required_columns: required_cols,
                time_range: None,
                existing_data_cids: vec![],
                budget,
            };

            let signal = state.feedback_store.compute_signal(&cid)?;
            let report = state.tcv_engine.evaluate(&metadata, &task, &signal);

            // Append community feedback summary
            let output = json!({
                "tcv": report,
                "community_feedback": {
                    "total_reviews": signal.total_reviews,
                    "avg_relevance": signal.avg_relevance,
                    "positive_rate": signal.positive_rate,
                    "negative_rate": signal.negative_rate,
                    "task_specific_signals": signal.task_signals,
                }
            });

            Ok(serde_json::to_string_pretty(&output)?)
        }

        // ---------------------------------------------------------------
        // dataset_feedback: submit on-chain usage attestation
        // ---------------------------------------------------------------
        "dataset_feedback" => {
            let cid_str = args.get("cid").and_then(|v| v.as_str()).unwrap_or("");
            let relevance = args.get("relevance_score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let quality = args.get("quality_rating").and_then(|v| v.as_u64()).unwrap_or(3) as u8;
            let success = args.get("task_success").and_then(|v| v.as_bool()).unwrap_or(true);
            let assessment_str = args
                .get("value_assessment")
                .and_then(|v| v.as_str())
                .unwrap_or("neutral");
            let task_type = args.get("task_type").and_then(|v| v.as_str()).unwrap_or("general");
            let task_desc = args
                .get("task_description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let comment = args.get("comment").and_then(|v| v.as_str()).map(String::from);

            let assessment = match assessment_str {
                "positive" => ValueAssessment::Positive,
                "negative" => ValueAssessment::Negative,
                _ => ValueAssessment::Neutral,
            };

            let feedback = DatasetFeedback {
                id: uuid::Uuid::new_v4().to_string(),
                dataset_cid: DatasetCid(cid_str.to_string()),
                agent_did: state.identity.did.clone(),
                task_type: task_type.to_string(),
                task_description: task_desc.to_string(),
                relevance_score: relevance.clamp(-1.0, 1.0),
                quality_rating: quality.clamp(1, 5),
                task_success: success,
                value_assessment: assessment,
                comment,
                timestamp: chrono::Utc::now(),
            };

            state.feedback_store.put(&feedback)?;

            // Show updated community signal after recording
            let cid = DatasetCid(cid_str.to_string());
            let signal = state.feedback_store.compute_signal(&cid)?;

            Ok(json!({
                "status": "recorded",
                "feedback_id": feedback.id,
                "on_chain": "EAS attestation simulated (Base L2)",
                "updated_community_signal": {
                    "total_reviews": signal.total_reviews,
                    "avg_relevance": format!("{:.2}", signal.avg_relevance),
                    "positive_rate": format!("{:.0}%", signal.positive_rate * 100.0),
                    "negative_rate": format!("{:.0}%", signal.negative_rate * 100.0),
                }
            })
            .to_string())
        }

        // ---------------------------------------------------------------
        // dataset_purchase: automated payment via x402 / MPP + file delivery
        // ---------------------------------------------------------------
        "dataset_purchase" => {
            let cid_str = args.get("cid").and_then(|v| v.as_str()).unwrap_or("");
            let max_price = args.get("max_price").and_then(|v| v.as_f64()).unwrap_or(0.0);

            let cid = DatasetCid(cid_str.to_string());
            let metadata = match state.store.get(&cid)? {
                Some(m) => m,
                None => anyhow::bail!("Dataset {cid_str} not found"),
            };

            if metadata.price.amount > max_price && max_price > 0.0 {
                anyhow::bail!(
                    "Price ${:.2} exceeds budget ${:.2}",
                    metadata.price.amount,
                    max_price
                );
            }

            // Auto-select payment protocol
            let (protocol, description) = if metadata.price.is_free() {
                ("none", "Free dataset — no payment required")
            } else if metadata.price.amount < 0.01 {
                ("x402", "Micropayment via x402 (USDC on Base L2)")
            } else if metadata.price.amount > 1.0 {
                ("erc8183_escrow", "Escrowed payment via ERC-8183 (verify → release)")
            } else {
                ("stripe_mpp", "Session payment via Stripe Machine Payment Protocol")
            };

            // File delivery: local file path if we published it, otherwise torrent download
            let delivery = match state.store.get_file_path(&cid)? {
                Some(path) if path.exists() => {
                    json!({
                        "method": "local",
                        "file_path": path.to_string_lossy(),
                        "size_bytes": std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                    })
                }
                _ => {
                    // Remote dataset: attempt torrent download to data_dir
                    let download_dir = state.store.get_file_path(
                        &DatasetCid("__config_data_dir__".into())
                    )?.unwrap_or_else(|| std::path::PathBuf::from("/tmp/guixu-downloads"));
                    let dest = download_dir.join(format!("{}.dat", &cid_str[..16.min(cid_str.len())]));
                    // TODO: real BitTorrent v2 download via torrent.rs
                    json!({
                        "method": "torrent_pending",
                        "info_hash": metadata.info_hash,
                        "download_path": dest.to_string_lossy(),
                        "note": "BitTorrent v2 download not yet implemented — use DHT peer to fetch",
                    })
                }
            };

            Ok(json!({
                "status": "purchased",
                "cid": cid_str,
                "price_paid": metadata.price.amount,
                "payment_protocol": protocol,
                "protocol_description": description,
                "tx_id": uuid::Uuid::new_v4().to_string(),
                "on_chain_receipt": "EAS attestation simulated (Base L2)",
                "delivery": delivery,
            })
            .to_string())
        }

        // ---------------------------------------------------------------
        // dataset_reviews: list all on-chain feedback for a dataset
        // ---------------------------------------------------------------
        "dataset_reviews" => {
            let cid_str = args.get("cid").and_then(|v| v.as_str()).unwrap_or("");
            let cid = DatasetCid(cid_str.to_string());

            let feedbacks = state.feedback_store.get_for_dataset(&cid)?;
            let signal = state.feedback_store.compute_signal(&cid)?;

            let reviews: Vec<serde_json::Value> = feedbacks
                .iter()
                .map(|fb| {
                    json!({
                        "feedback_id": fb.id,
                        "agent": fb.agent_did.0,
                        "task_type": fb.task_type,
                        "task_description": fb.task_description,
                        "relevance_score": fb.relevance_score,
                        "quality_rating": fb.quality_rating,
                        "task_success": fb.task_success,
                        "value_assessment": fb.value_assessment,
                        "comment": fb.comment,
                        "timestamp": fb.timestamp.to_rfc3339(),
                    })
                })
                .collect();

            Ok(json!({
                "cid": cid_str,
                "total_reviews": signal.total_reviews,
                "summary": {
                    "avg_relevance": signal.avg_relevance,
                    "avg_quality": signal.avg_quality,
                    "positive_rate": signal.positive_rate,
                    "negative_rate": signal.negative_rate,
                    "task_breakdown": signal.task_signals,
                },
                "reviews": reviews,
            })
            .to_string())
        }

        // ---------------------------------------------------------------
        // dataset_verify
        // ---------------------------------------------------------------
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

        // ---------------------------------------------------------------
        // dataset_publish
        // ---------------------------------------------------------------
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
