// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use crate::client::ValuationReportResponse;
use data_core::types::*;

/// Verify a valuation report's proof digest and consistency.
/// V1: checks digest is non-empty and verdict is "verified".
/// Future: actual Groth16 proof verification with arkworks.
pub fn verify_report(report: &ValuationReportResponse) -> ProofVerdict {
    let verdict_str = report.proof_verdict.as_deref().unwrap_or("pending");
    let has_digest = report
        .proof_digest
        .as_ref()
        .map(|d| !d.is_empty())
        .unwrap_or(false);

    if verdict_str == "verified" && has_digest {
        ProofVerdict::Verified
    } else if verdict_str == "failed" {
        ProofVerdict::Failed
    } else {
        ProofVerdict::Pending
    }
}

/// Build ConfidentialValuationEvidence from a verified report.
pub fn build_evidence(
    report: &ValuationReportResponse,
    proof_system: &str,
) -> Option<ConfidentialValuationEvidence> {
    let verdict = verify_report(report);
    if verdict != ProofVerdict::Verified {
        return None;
    }

    let risk_flags: Vec<String> = report
        .risk_flags
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let score_band = report
        .score_components
        .as_ref()
        .and_then(|c| c.get("score_band"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let roi_band = report
        .score_components
        .as_ref()
        .and_then(|c| c.get("roi_band"))
        .and_then(|v| v.as_str())
        .map(String::from);

    Some(ConfidentialValuationEvidence {
        report_id: report.id.clone().unwrap_or_default(),
        report_digest: report.proof_digest.clone().unwrap_or_default(),
        manifest_digest: None,
        dataset_commitment_digest: None,
        proof_system: proof_system.into(),
        proof_verdict: verdict,
        score: report.score,
        score_band,
        recommendation: report.recommendation.clone(),
        risk_flags,
        roi_band,
    })
}
