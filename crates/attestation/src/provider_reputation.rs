// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Provider reputation computation from aggregated community signals.
//!
//! This module provides a unified way to compute provider-level reputation scores
//! by aggregating signals across multiple datasets published by the same provider.

use data_core::feedback::CommunitySignal;

/// Compute a provider-level reputation score by aggregating CommunitySignals
/// across all datasets published by the provider.
///
/// Returns (score 0-100, total_reviews, avg_quality).
///
/// The score is computed as a weighted blend of:
/// - Relevance score: (-1..1 normalized to 0..100) × 0.4
/// - Quality score: (1..5 normalized to 20..100) × 0.3
/// - Sentiment score: (positive_rate - negative_rate normalized) × 0.3
pub fn compute_provider_reputation(signals: &[CommunitySignal]) -> (f64, u64, f64) {
    let mut total_reviews = 0u64;
    let mut weighted_relevance = 0.0;
    let mut weighted_quality = 0.0;
    let mut total_positive = 0u64;
    let mut total_negative = 0u64;

    for signal in signals {
        let n = signal.total_reviews;
        if n == 0 {
            continue;
        }
        total_reviews += n;
        weighted_relevance += signal.avg_relevance * n as f64;
        weighted_quality += signal.avg_quality * n as f64;
        total_positive += (signal.positive_rate * n as f64) as u64;
        total_negative += (signal.negative_rate * n as f64) as u64;
    }

    if total_reviews == 0 {
        return (50.0, 0, 0.0);
    }

    let avg_relevance = weighted_relevance / total_reviews as f64;
    let avg_quality = weighted_quality / total_reviews as f64;
    let positive_rate = total_positive as f64 / total_reviews as f64;
    let negative_rate = total_negative as f64 / total_reviews as f64;

    let relevance_score = (avg_relevance + 1.0) * 50.0;
    let quality_score = avg_quality * 20.0;
    let sentiment_score = (positive_rate - negative_rate + 1.0) * 50.0;

    let score = relevance_score * 0.4 + quality_score * 0.3 + sentiment_score * 0.3;
    (score.clamp(0.0, 100.0), total_reviews, avg_quality)
}

#[cfg(test)]
mod tests {
    use super::*;
    use data_core::feedback::{CommunitySignal, TaskSignal, ValueAssessment};

    fn make_signal(
        dataset_cid: &str,
        reviews: u64,
        pos_rate: f64,
        neg_rate: f64,
        avg_rel: f64,
        avg_qual: f64,
    ) -> CommunitySignal {
        CommunitySignal {
            dataset_cid: data_core::types::DatasetCid(dataset_cid.to_string()),
            total_reviews: reviews,
            avg_relevance: avg_rel,
            avg_quality: avg_qual,
            positive_rate: pos_rate,
            negative_rate: neg_rate,
            task_signals: vec![],
        }
    }

    #[test]
    fn test_no_signals_returns_default() {
        let (score, reviews, avg_qual) = compute_provider_reputation(&[]);
        assert_eq!(score, 50.0);
        assert_eq!(reviews, 0);
        assert_eq!(avg_qual, 0.0);
    }

    #[test]
    fn test_single_signal() {
        let signals = vec![make_signal("cid-1", 10, 0.8, 0.1, 0.5, 4.0)];
        let (score, reviews, avg_qual) = compute_provider_reputation(&signals);
        assert_eq!(reviews, 10);
        assert_eq!(avg_qual, 4.0);
        assert!(score > 50.0);
    }

    #[test]
    fn test_multiple_signals_weighted() {
        let signals = vec![
            make_signal("cid-1", 10, 0.9, 0.0, 0.8, 4.5),
            make_signal("cid-2", 90, 0.6, 0.2, 0.2, 3.5),
        ];
        let (score, reviews, avg_qual) = compute_provider_reputation(&signals);
        assert_eq!(reviews, 100);
        // Should be weighted towards the larger dataset
        assert!(avg_qual > 3.5 && avg_qual < 4.5);
        assert!(score > 50.0);
    }

    #[test]
    fn test_all_negative() {
        let signals = vec![make_signal("cid-1", 10, 0.0, 0.9, -0.7, 2.0)];
        let (score, _, _) = compute_provider_reputation(&signals);
        assert!(score < 50.0);
    }
}
