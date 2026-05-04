// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use data_core::types::*;

/// Merge confidential valuation evidence into a proxy utility score.
/// Returns an adjusted score that can be fed into TCV/ROI.
pub fn merge_into_proxy_score(base_score: f64, evidence: &ConfidentialValuationEvidence) -> f64 {
    if evidence.proof_verdict != ProofVerdict::Verified {
        return base_score; // Don't adjust if proof not verified
    }

    // Use the verified score directly if available, weighted with base
    match evidence.score {
        Some(verified_score) => {
            // 70% weight to verified score, 30% to metadata-based base score
            0.7 * verified_score + 0.3 * base_score
        }
        None => base_score,
    }
}

/// Determine if a dataset is worth purchasing based on evidence.
pub fn should_purchase(
    evidence: &ConfidentialValuationEvidence,
    max_price: f64,
    price: f64,
) -> bool {
    if evidence.proof_verdict != ProofVerdict::Verified {
        return false;
    }
    if evidence.recommendation.as_deref() == Some("skip") {
        return false;
    }
    if price > max_price {
        return false;
    }

    matches!(evidence.score, Some(s) if s >= 50.0)
}
