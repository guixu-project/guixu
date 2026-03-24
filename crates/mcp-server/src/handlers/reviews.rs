use anyhow::Result;
use serde_json::json;

use data_core::types::DatasetCid;

use crate::server::AppState;

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
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
