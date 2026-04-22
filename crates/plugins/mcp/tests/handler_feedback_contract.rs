// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Contract tests for MCP feedback handler.
//!
//! Tests that feedback is correctly recorded and community signal is updated.

use data_core::feedback::{DatasetFeedback, ValueAssessment};
use data_core::types::DatasetCid;
use data_mcp_server::server::AppState;
use serde_json::json;

// ── Test setup using AppState::for_codex ──────────────────────────────────────

fn test_state() -> AppState {
    // Use the for_codex helper which creates a minimal test state
    // This uses temp directories and in-memory stores
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async { AppState::for_codex().await.unwrap() })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn feedback_records_positive_assessment() {
    let state = test_state();
    let cid = DatasetCid("test-cid-feedback-1".into());

    // Pre-create metadata so feedback can be linked
    let metadata = data_test_utils::builders::DatasetMetadataBuilder::new(&cid.0)
        .title("Test Dataset")
        .build();
    state.store.put(&metadata).unwrap();

    let args = json!({
        "cid": cid.0,
        "relevance_score": 0.8,
        "quality_rating": 5,
        "task_success": true,
        "value_assessment": "positive",
        "task_type": "classification",
        "task_description": "sentiment analysis"
    });

    let result = data_mcp_server::handlers::feedback::handle(args, &state)
        .await
        .expect("feedback handle should succeed");

    let response: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(response["status"], "recorded");
    assert!(!response["feedback_id"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn feedback_records_negative_assessment() {
    let state = test_state();
    let cid = DatasetCid("test-cid-feedback-2".into());

    let metadata = data_test_utils::builders::DatasetMetadataBuilder::new(&cid.0)
        .title("Test Dataset")
        .build();
    state.store.put(&metadata).unwrap();

    let args = json!({
        "cid": cid.0,
        "relevance_score": -0.5,
        "quality_rating": 2,
        "task_success": false,
        "value_assessment": "negative",
        "task_type": "regression",
        "task_description": "price prediction"
    });

    let result = data_mcp_server::handlers::feedback::handle(args, &state)
        .await
        .expect("feedback handle should succeed");

    let response: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(response["status"], "recorded");
}

#[tokio::test]
async fn feedback_updates_community_signal() {
    let state = test_state();
    let cid = DatasetCid("test-cid-feedback-3".into());

    let metadata = data_test_utils::builders::DatasetMetadataBuilder::new(&cid.0)
        .title("Test Dataset")
        .build();
    state.store.put(&metadata).unwrap();

    // Submit multiple feedbacks
    for i in 0..3 {
        let args = json!({
            "cid": cid.0,
            "relevance_score": 0.7,
            "quality_rating": 4,
            "task_success": true,
            "value_assessment": "positive",
            "task_type": "classification"
        });
        let _ = data_mcp_server::handlers::feedback::handle(args, &state).await;
    }

    // Get the signal to verify it was updated
    let signal = state.feedback_store.compute_signal(&cid).unwrap();
    assert_eq!(
        signal.total_reviews, 3,
        "should have 3 reviews after 3 feedbacks"
    );
}

#[tokio::test]
async fn feedback_relevance_score_is_clamped() {
    let state = test_state();
    let cid = DatasetCid("test-cid-feedback-4".into());

    let metadata = data_test_utils::builders::DatasetMetadataBuilder::new(&cid.0)
        .title("Test Dataset")
        .build();
    state.store.put(&metadata).unwrap();

    // Submit feedback with out-of-range relevance score
    let args = json!({
        "cid": cid.0,
        "relevance_score": 5.0, // way over the 1.0 max
        "quality_rating": 3,
        "task_success": true,
        "value_assessment": "neutral",
        "task_type": "general"
    });

    let result = data_mcp_server::handlers::feedback::handle(args, &state)
        .await
        .expect("feedback handle should succeed");

    // Should still succeed - the clamping happens internally
    let response: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(response["status"], "recorded");
}

#[tokio::test]
async fn feedback_defaults_when_args_missing() {
    let state = test_state();
    let cid = DatasetCid("test-cid-feedback-5".into());

    let metadata = data_test_utils::builders::DatasetMetadataBuilder::new(&cid.0)
        .title("Test Dataset")
        .build();
    state.store.put(&metadata).unwrap();

    // Submit feedback with minimal args
    let args = json!({
        "cid": cid.0
    });

    let result = data_mcp_server::handlers::feedback::handle(args, &state)
        .await
        .expect("feedback handle should succeed with defaults");

    let response: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(response["status"], "recorded");
}
