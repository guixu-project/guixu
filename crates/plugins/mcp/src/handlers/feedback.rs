// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde_json::json;

use data_core::feedback::{DatasetFeedback, ValueAssessment};
use data_core::types::DatasetCid;

use crate::server::AppState;

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
    let cid_str = args.get("cid").and_then(|v| v.as_str()).unwrap_or("");
    let relevance = args
        .get("relevance_score")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let quality = args
        .get("quality_rating")
        .and_then(|v| v.as_u64())
        .unwrap_or(3) as u8;
    let success = args
        .get("task_success")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let assessment_str = args
        .get("value_assessment")
        .and_then(|v| v.as_str())
        .unwrap_or("neutral");
    let task_type = args
        .get("task_type")
        .and_then(|v| v.as_str())
        .unwrap_or("general");
    let task_desc = args
        .get("task_description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let comment = args
        .get("comment")
        .and_then(|v| v.as_str())
        .map(String::from);

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
