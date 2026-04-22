// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use data_confidential_valuation::client::ValuationReportResponse;
use data_confidential_valuation::merge::{merge_into_proxy_score, should_purchase};
use data_confidential_valuation::verify::{build_evidence, verify_report};
use data_core::types::ProofVerdict;

fn mock_report(verdict: &str, digest: Option<&str>, score: Option<f64>) -> ValuationReportResponse {
    ValuationReportResponse {
        id: Some("report-1".into()),
        request_id: Some("req-1".into()),
        score,
        score_components: Some(serde_json::json!({"score_band": "positive", "roi_band": "high"})),
        recommendation: Some("buy".into()),
        risk_flags: Some(serde_json::json!([])),
        proof_digest: digest.map(String::from),
        proof_verdict: Some(verdict.into()),
        status: "completed".into(),
    }
}

// ============================================================================
// Verify module tests
// ============================================================================

#[test]
fn test_verify_verified_report() {
    let r = mock_report("verified", Some("0xabc123"), Some(75.0));
    assert_eq!(verify_report(&r), ProofVerdict::Verified);
}

#[test]
fn test_verify_failed_report() {
    let r = mock_report("failed", Some("0xabc"), None);
    assert_eq!(verify_report(&r), ProofVerdict::Failed);
}

#[test]
fn test_verify_pending_report() {
    let r = mock_report("pending", None, None);
    assert_eq!(verify_report(&r), ProofVerdict::Pending);
}

#[test]
fn test_verify_no_digest_is_pending() {
    let r = mock_report("verified", None, Some(80.0));
    assert_eq!(verify_report(&r), ProofVerdict::Pending);
}

#[test]
fn test_build_evidence_verified() {
    let r = mock_report("verified", Some("0xdigest"), Some(72.0));
    let ev = build_evidence(&r, "groth16_arkworks").unwrap();
    assert_eq!(ev.proof_verdict, ProofVerdict::Verified);
    assert_eq!(ev.score, Some(72.0));
    assert_eq!(ev.report_id, "report-1");
    assert_eq!(ev.proof_system, "groth16_arkworks");
    assert_eq!(ev.recommendation, Some("buy".into()));
}

#[test]
fn test_build_evidence_unverified_returns_none() {
    let r = mock_report("failed", Some("0x"), None);
    assert!(build_evidence(&r, "groth16_arkworks").is_none());
}

// ============================================================================
// Merge module tests
// ============================================================================

#[test]
fn test_merge_verified_score() {
    let r = mock_report("verified", Some("0xd"), Some(80.0));
    let ev = build_evidence(&r, "groth16").unwrap();
    let merged = merge_into_proxy_score(50.0, &ev);
    // 0.7 * 80 + 0.3 * 50 = 56 + 15 = 71
    assert!((merged - 71.0).abs() < 0.01);
}

#[test]
fn test_merge_no_score_keeps_base() {
    let mut r = mock_report("verified", Some("0xd"), None);
    let ev = build_evidence(&r, "groth16").unwrap();
    let merged = merge_into_proxy_score(60.0, &ev);
    assert_eq!(merged, 60.0);
}

#[test]
fn test_should_purchase_positive() {
    let r = mock_report("verified", Some("0xd"), Some(70.0));
    let ev = build_evidence(&r, "groth16").unwrap();
    assert!(should_purchase(&ev, 10.0, 2.0));
}

#[test]
fn test_should_purchase_low_score() {
    let r = mock_report("verified", Some("0xd"), Some(30.0));
    let ev = build_evidence(&r, "groth16").unwrap();
    assert!(!should_purchase(&ev, 10.0, 2.0));
}

#[test]
fn test_should_purchase_over_budget() {
    let r = mock_report("verified", Some("0xd"), Some(90.0));
    let ev = build_evidence(&r, "groth16").unwrap();
    assert!(!should_purchase(&ev, 5.0, 10.0));
}

#[test]
fn test_should_purchase_skip_recommendation() {
    let mut r = mock_report("verified", Some("0xd"), Some(80.0));
    r.recommendation = Some("skip".into());
    let ev = build_evidence(&r, "groth16").unwrap();
    assert!(!should_purchase(&ev, 10.0, 2.0));
}
