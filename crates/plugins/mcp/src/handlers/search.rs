// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use data_core::feedback::CommunitySignal;
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::{AccessMode, DatasetCid, SkillCapability, SourceFamily};
use data_search::engine::{RankedResult, SearchFilters, SignalFetcher};
use data_search::intent::QueryProfile;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use super::trace_hooks::with_trace;
use crate::discovery::types::{DiscoverySearchRequest, WorkspaceMetaOutput};
use crate::server::AppState;

struct ParsedSearchRequest {
    query: String,
    task_type: Option<String>,
    limit: usize,
    filters: SearchFilters,
}

fn parse_string_array(obj: &Value, plural_key: &str, singular_key: &str) -> Vec<String> {
    obj.get(plural_key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect()
        })
        .or_else(|| {
            obj.get(singular_key)
                .and_then(|value| value.as_str())
                .map(|value| vec![value.to_string()])
        })
        .unwrap_or_default()
}

fn parse_enum_array<T>(obj: &Value, plural_key: &str, singular_key: &str) -> Vec<T>
where
    T: DeserializeOwned,
{
    obj.get(plural_key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|value| serde_json::from_value(value.clone()).ok())
                .collect()
        })
        .or_else(|| {
            obj.get(singular_key)
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok())
                .map(|value| vec![value])
        })
        .unwrap_or_default()
}

fn parse_request(args: &Value) -> ParsedSearchRequest {
    let query = args
        .get("query")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let task_type = args
        .get("task_type")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(10) as usize;

    let filter_obj = args.get("filters").cloned().unwrap_or_default();
    let filters = SearchFilters {
        topic: filter_obj
            .get("topic")
            .and_then(|value| value.as_str())
            .map(String::from),
        min_rows: filter_obj.get("min_rows").and_then(|value| value.as_u64()),
        max_price: filter_obj.get("max_price").and_then(|value| value.as_f64()),
        license: filter_obj
            .get("license")
            .and_then(|value| value.as_str())
            .map(String::from),
        min_quality: filter_obj
            .get("min_quality")
            .and_then(|value| value.as_f64()),
        skill_ids: parse_string_array(&filter_obj, "skill_ids", "skill_id"),
        source_families: parse_enum_array::<SourceFamily>(
            &filter_obj,
            "source_families",
            "source_family",
        ),
        required_capabilities: parse_enum_array::<SkillCapability>(
            &filter_obj,
            "required_capabilities",
            "required_capability",
        ),
        chain: filter_obj
            .get("chain")
            .and_then(|value| value.as_str())
            .map(String::from),
        protocol: filter_obj
            .get("protocol")
            .and_then(|value| value.as_str())
            .map(String::from),
        asset: filter_obj
            .get("asset")
            .and_then(|value| value.as_str())
            .map(String::from),
        category: filter_obj
            .get("category")
            .and_then(|value| value.as_str())
            .map(String::from),
        free_only: filter_obj
            .get("free_only")
            .and_then(|value| value.as_bool()),
    };

    ParsedSearchRequest {
        query,
        task_type,
        limit,
        filters,
    }
}

pub async fn handle(args: Value, state: &AppState) -> Result<String> {
    with_trace(&state.trace_manager, "mcp.search", None, None, async {
        inner_handle(args, state).await
    })
    .await
}

async fn inner_handle(args: Value, state: &AppState) -> Result<String> {
    let request = parse_request(&args);
    if state.search_workers == 0 {
        legacy_handle(&request, state).await
    } else {
        agentic_handle(&request, state).await
    }
}

async fn legacy_handle(request: &ParsedSearchRequest, state: &AppState) -> Result<String> {
    let profile = build_legacy_profile(&request.query, request.task_type.as_deref());
    let local_metadata = state.store.list_all()?;
    let signal_fetcher = build_signal_fetcher(state);

    let search_output = state
        .search_engine
        .search_with_profile(
            &profile,
            &request.filters,
            &local_metadata,
            &signal_fetcher,
            request.limit,
        )
        .await?;

    persist_search_results(state, &search_output.results)?;

    let response = json!({
        "results": format_results(&search_output.results),
        "errors": search_output.errors,
    });
    Ok(serde_json::to_string_pretty(&response)?)
}

async fn agentic_handle(request: &ParsedSearchRequest, state: &AppState) -> Result<String> {
    let runtime = state
        .discovery_runtime
        .as_ref()
        .ok_or_else(|| {
            anyhow!(
                "agentic dataset_search is required because GUIXU_SEARCH_WORKERS > 0, but the discovery runtime is unavailable"
            )
        })?;
    let search_output = runtime
        .run_search(DiscoverySearchRequest {
            raw_query: request.query.clone(),
            filters: request.filters.clone(),
            limit: request.limit,
        })
        .await?;

    persist_search_results(state, &search_output.results)?;

    let workspace_meta = WorkspaceMetaOutput {
        workspace_id: search_output.workspace.workspace_id.clone(),
        worker_count: search_output.workspace.workers.len(),
        observation_count: search_output.workspace.observations.len(),
        mode: "parallel_subagents".to_string(),
    };
    let response = json!({
        "results": format_results(&search_output.results),
        "errors": search_output.errors,
        "intent": search_output.intent,
        "workspace_meta": workspace_meta,
        "workspace": search_output.workspace,
    });
    Ok(serde_json::to_string_pretty(&response)?)
}

fn build_legacy_profile(query: &str, task_type: Option<&str>) -> QueryProfile {
    let keywords: Vec<String> = query
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|character: char| !character.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| word.len() > 1)
        .take(10)
        .collect();

    QueryProfile {
        raw_query: query.to_string(),
        task_type: task_type.map(String::from),
        task_description: Some(query.to_string()),
        target_entity: None,
        keywords,
        data_standard: Default::default(),
        user_profile: Default::default(),
    }
}

fn build_signal_fetcher(state: &AppState) -> SignalFetcher {
    let feedback_store = state.feedback_store.clone();
    Box::new(move |cid_str: &str| {
        let cid = DatasetCid(cid_str.to_string());
        feedback_store
            .compute_signal(&cid)
            .unwrap_or_else(|_| CommunitySignal {
                dataset_cid: cid,
                total_reviews: 0,
                avg_relevance: 0.0,
                avg_quality: 0.0,
                positive_rate: 0.0,
                negative_rate: 0.0,
                task_signals: vec![],
            })
    })
}

fn persist_search_results(state: &AppState, results: &[RankedResult]) -> Result<()> {
    for ranked in results {
        let result = &ranked.result;
        if state.store.get(&result.cid).ok().flatten().is_some() {
            continue;
        }

        let metadata = DatasetMetadata {
            cid: result.cid.clone(),
            info_hash: None,
            title: result.title.clone(),
            description: result.description.clone(),
            tags: result.tags.clone(),
            data_type: result.data_type,
            schema: result.schema.clone(),
            stats: None,
            video_meta: None,
            access: if result.price.is_free() {
                AccessMode::Open
            } else {
                AccessMode::Paid
            },
            price: result.price.clone(),
            license: result.license.clone(),
            provider: result.provider.clone(),
            signature: String::new(),
            provenance: Provenance::Original,
            created_at: result.created_at,
            updated_at: result.created_at,
            verifiable_credential: None,
            source_attributes: result.source_attributes.clone(),
        };
        let _ = state.store.put(&metadata);
    }

    Ok(())
}

fn format_results(results: &[RankedResult]) -> Vec<Value> {
    results
        .iter()
        .enumerate()
        .map(|(index, ranked)| {
            json!({
                "rank": index + 1,
                "cid": ranked.result.cid.0,
                "title": ranked.result.title,
                "description": ranked.result.description,
                "source": ranked.result.source,
                "data_type": ranked.result.data_type,
                "price": ranked.result.price,
                "schema": {
                    "columns": ranked.result.schema.columns.len(),
                    "rows": ranked.result.schema.row_count,
                    "size_bytes": ranked.result.schema.size_bytes,
                },
                "rank_score": format!("{:.1}", ranked.rank_score),
                "community": {
                    "total_reviews": ranked.signal.total_reviews,
                    "avg_relevance": format!("{:.2}", ranked.signal.avg_relevance),
                    "positive_rate": format!("{:.0}%", ranked.signal.positive_rate * 100.0),
                    "negative_rate": format!("{:.0}%", ranked.signal.negative_rate * 100.0),
                },
                "provider_meta": ranked.result.provider_meta,
                "governance": ranked.result.governance,
                "source_attributes": ranked.result.source_attributes,
            })
        })
        .collect()
}
