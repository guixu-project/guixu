// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! MCP handler for querying agent memory mutation history.

use anyhow::Result;
use serde_json::json;

use super::trace_hooks::with_trace;
use crate::state::AppState;

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
    with_trace(
        &state.trace_manager,
        "mcp.memory_history",
        None,
        None,
        inner_handle(args, state),
    )
    .await
}

async fn inner_handle(args: serde_json::Value, state: &AppState) -> Result<String> {
    let memory_key = args
        .get("memory_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);

    if memory_key.is_empty() {
        anyhow::bail!("memory_key is required");
    }

    let tm = state
        .trace_manager
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("tracing is not enabled"))?;
    let tm = tm.read().await;
    if !tm.is_enabled() {
        anyhow::bail!("tracing is not enabled");
    }
    drop(tm);

    let db_path = {
        let cfg = data_core::config::NodeConfig::load_or_default();
        cfg.trace.db_path.clone()
    };

    let key = memory_key.clone();
    let spans = tokio::task::spawn_blocking(move || {
        let store = data_storage::trace_store::TraceStore::open(std::path::Path::new(&db_path))?;
        store.memory_timeline(&key, None, limit)
    })
    .await??;

    let mutations: Vec<serde_json::Value> = spans
        .iter()
        .map(|s| {
            json!({
                "trace_id": s.trace_id,
                "span_id": s.span_id,
                "timestamp": s.start_time.to_rfc3339(),
                "mutation_kind": s.attributes.get("mutation_kind"),
                "diff": s.attributes.get("diff"),
                "memory_key": s.attributes.get("memory_key"),
            })
        })
        .collect();

    Ok(json!({
        "memory_key": memory_key,
        "mutation_count": mutations.len(),
        "mutations": mutations,
    })
    .to_string())
}
