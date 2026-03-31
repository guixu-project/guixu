use anyhow::Result;
use serde_json::json;

use data_core::feedback::CommunitySignal;
use data_core::types::DatasetCid;
use data_search::engine::SearchFilters;

use crate::server::AppState;

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
    let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let task_type = args
        .get("task_type")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    let filter_obj = args.get("filters").cloned().unwrap_or_default();
    let filters = SearchFilters {
        topic: filter_obj
            .get("topic")
            .and_then(|v| v.as_str())
            .map(String::from),
        min_rows: filter_obj.get("min_rows").and_then(|v| v.as_u64()),
        max_price: filter_obj.get("max_price").and_then(|v| v.as_f64()),
        license: filter_obj
            .get("license")
            .and_then(|v| v.as_str())
            .map(String::from),
        min_quality: filter_obj.get("min_quality").and_then(|v| v.as_f64()),
        source: filter_obj
            .get("source")
            .and_then(|v| v.as_str())
            .map(String::from),
        chain: filter_obj
            .get("chain")
            .and_then(|v| v.as_str())
            .map(String::from),
        protocol: filter_obj
            .get("protocol")
            .and_then(|v| v.as_str())
            .map(String::from),
        asset: filter_obj
            .get("asset")
            .and_then(|v| v.as_str())
            .map(String::from),
        category: filter_obj
            .get("category")
            .and_then(|v| v.as_str())
            .map(String::from),
        free_only: filter_obj.get("free_only").and_then(|v| v.as_bool()),
    };

    let local_metadata = state.store.list_all()?;

    let fb_store = state.feedback_store.clone();
    let signal_fetcher: data_search::engine::SignalFetcher = Box::new(move |cid_str: &str| {
        let cid = DatasetCid(cid_str.to_string());
        fb_store
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
    });

    let search_output = state
        .search_engine
        .search_with_task_type(
            query,
            task_type,
            &filters,
            &local_metadata,
            &signal_fetcher,
            limit,
        )
        .await?;

    // Persist external search results so downstream tools (evaluate, purchase)
    // can look them up by CID without requiring catalog sync.
    for ranked in &search_output.results {
        let r = &ranked.result;
        if state.store.get(&r.cid).ok().flatten().is_none() {
            let metadata = data_core::metadata::DatasetMetadata {
                cid: r.cid.clone(),
                info_hash: None,
                title: r.title.clone(),
                description: r.description.clone(),
                tags: r.tags.clone(),
                data_type: r.data_type,
                schema: r.schema.clone(),
                stats: None,
                video_meta: None,
                access: if r.price.is_free() {
                    data_core::types::AccessMode::Open
                } else {
                    data_core::types::AccessMode::Paid
                },
                price: r.price.clone(),
                license: r.license.clone(),
                provider: r.provider.clone(),
                signature: String::new(),
                provenance: data_core::metadata::Provenance::Original,
                created_at: r.created_at,
                updated_at: r.created_at,
                verifiable_credential: None,
                source_attributes: r.source_attributes.clone(),
            };
            let _ = state.store.put(&metadata);
        }
    }

    let output: Vec<serde_json::Value> = search_output
        .results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            json!({
                "rank": i + 1,
                "cid": r.result.cid.0,
                "title": r.result.title,
                "description": r.result.description,
                "tags": r.result.tags,
                "source": r.result.source,
                "data_type": r.result.data_type,
                "price": r.result.price,
                "schema": {
                    "columns": r.result.schema.columns.iter().map(|column| column.name.clone()).collect::<Vec<_>>(),
                    "column_count": r.result.schema.columns.len(),
                    "row_count": r.result.schema.row_count,
                    "size_bytes": r.result.schema.size_bytes,
                },
                "market": r.result.market,
                "rank_score": format!("{:.1}", r.rank_score),
                "community": {
                    "total_reviews": r.signal.total_reviews,
                    "avg_relevance": format!("{:.2}", r.signal.avg_relevance),
                    "positive_rate": format!("{:.0}%", r.signal.positive_rate * 100.0),
                    "negative_rate": format!("{:.0}%", r.signal.negative_rate * 100.0),
                },
                "source_attributes": r.result.source_attributes,
            })
        })
        .collect();

    let mut response = json!({
        "results": output,
        "errors": search_output.errors,
    });

    if let Some(profile) = &search_output.profile {
        response["intent"] = json!({
            "task_type": profile.task_type,
            "task_description": profile.task_description,
            "target_entity": profile.target_entity,
            "keywords": profile.keywords,
            "data_standard": profile.data_standard,
        });
    }

    Ok(serde_json::to_string_pretty(&response)?)
}
