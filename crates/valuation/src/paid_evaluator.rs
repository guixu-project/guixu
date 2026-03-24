use anyhow::Result;
use data_core::feedback::CommunitySignal;
use data_core::metadata::DatasetMetadata;
use data_core::types::QualityScore;
use serde::{Deserialize, Serialize};

/// Evaluates paid datasets by ROI — is it worth paying for?
/// Incorporates on-chain community feedback into the assessment.
pub struct PaidDataEvaluator;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoiReport {
    pub estimated_value: f64,
    pub asking_price: f64,
    pub roi_ratio: f64,
    pub has_free_alternative: bool,
    pub best_free_quality: Option<f64>,
    pub scarcity_premium: f64,
    pub community_confidence: f64,
    pub previous_buyer_success_rate: Option<f64>,
    pub recommendation: String,
}

impl PaidDataEvaluator {
    /// Assess whether a paid dataset is worth purchasing,
    /// using both quality metrics and on-chain community feedback.
    pub async fn evaluate(
        &self,
        metadata: &DatasetMetadata,
        quality: &QualityScore,
        free_alternatives: &[(&DatasetMetadata, &QualityScore)],
        signal: &CommunitySignal,
    ) -> Result<RoiReport> {
        let asking_price = metadata.price.amount;

        let best_free = free_alternatives
            .iter()
            .max_by(|a, b| a.1.total.partial_cmp(&b.1.total).unwrap());

        let has_free_alternative = best_free.is_some();
        let best_free_quality = best_free.map(|(_, q)| q.total);

        // Scarcity: fewer alternatives → higher value
        let scarcity_premium = if free_alternatives.is_empty() {
            1.5
        } else {
            1.0 / (1.0 + free_alternatives.len() as f64 * 0.2)
        };

        // Community confidence: how much do previous buyers trust this dataset?
        let community_confidence = if signal.total_reviews > 0 {
            signal.positive_rate
        } else {
            0.5 // neutral when no reviews
        };

        let previous_buyer_success_rate = if signal.total_reviews > 0 {
            Some(
                signal
                    .task_signals
                    .iter()
                    .map(|ts| ts.success_rate * ts.count as f64)
                    .sum::<f64>()
                    / signal.total_reviews as f64,
            )
        } else {
            None
        };

        // Quality premium over best free alternative, boosted by community signal
        let quality_delta = best_free_quality
            .map(|fq| (quality.total - fq).max(0.0))
            .unwrap_or(quality.total);

        let estimated_value =
            quality_delta * 0.01 * scarcity_premium * (0.5 + community_confidence * 0.5);
        let roi_ratio = if asking_price > 0.0 {
            estimated_value / asking_price
        } else {
            f64::INFINITY
        };

        let recommendation = if signal.negative_rate > 0.3 {
            format!(
                "⚠️ Caution — {:.0}% negative reviews from previous buyers",
                signal.negative_rate * 100.0
            )
        } else if roi_ratio > 2.0 {
            let mut msg = format!("Strong buy (ROI = {roi_ratio:.1}x)");
            if let Some(sr) = previous_buyer_success_rate {
                msg.push_str(&format!(". {:.0}% of previous buyers succeeded", sr * 100.0));
            }
            msg
        } else if roi_ratio > 1.0 {
            format!("Buy (ROI = {roi_ratio:.1}x)")
        } else if has_free_alternative {
            format!(
                "Skip — free alternative available (quality {:.0})",
                best_free_quality.unwrap_or(0.0)
            )
        } else {
            format!("Marginal (ROI = {roi_ratio:.1}x)")
        };

        Ok(RoiReport {
            estimated_value,
            asking_price,
            roi_ratio,
            has_free_alternative,
            best_free_quality,
            scarcity_premium,
            community_confidence,
            previous_buyer_success_rate,
            recommendation,
        })
    }
}
