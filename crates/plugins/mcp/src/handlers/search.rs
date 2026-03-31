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
                "source": r.result.source,
                "data_type": r.result.data_type,
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
                },
                "source_attributes": r.result.source_attributes,
            })
        })
        .collect();

    let response = json!({
        "results": output,
        "errors": search_output.errors,
    });

    Ok(serde_json::to_string_pretty(&response)?)
}
