use anyhow::Result;
use serde_json::json;

use data_search::adapters::pan_search::PanSearchAdapter;
use data_search::adapters::ExternalAdapter;

use crate::server::AppState;

pub async fn handle(args: serde_json::Value, _state: &AppState) -> Result<String> {
    let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let platform = args.get("platform").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    let adapter = PanSearchAdapter::default();
    let mut results = adapter.search(query, limit * 2).await?;

    // Filter by platform if specified
    if let Some(plat) = platform {
        results.retain(|r| {
            r.source_attributes
                .as_ref()
                .and_then(|a| a.get("platform"))
                .and_then(|v| v.as_str())
                .map(|p| p.eq_ignore_ascii_case(plat))
                .unwrap_or(false)
        });
    }
    results.truncate(limit);

    let output: Vec<serde_json::Value> = results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let attrs = r.source_attributes.as_ref();
            json!({
                "rank": i + 1,
                "title": r.title,
                "platform": attrs.and_then(|a| a.get("platform")),
                "share_url": attrs.and_then(|a| a.get("share_url")),
                "access_code": attrs.and_then(|a| a.get("code")),
                "source": attrs.and_then(|a| a.get("origin_source")),
                "snapshot_time": attrs.and_then(|a| a.get("snapshot_time")),
                "data_type": r.data_type,
            })
        })
        .collect();

    let response = json!({
        "query": query,
        "total": output.len(),
        "results": output,
    });

    Ok(serde_json::to_string_pretty(&response)?)
}
