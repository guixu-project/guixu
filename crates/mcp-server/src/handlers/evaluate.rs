use anyhow::Result;
use serde_json::json;

use data_core::types::DatasetCid;
use data_valuation::tcv::TaskContext;

use crate::server::AppState;

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
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
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
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
