use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use data_core::types::QualityScore;
use serde::{Deserialize, Serialize};

/// Evaluates paid datasets by ROI — is it worth paying for?
pub struct PaidDataEvaluator;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoiReport {
    pub estimated_value: f64,
    pub asking_price: f64,
    pub roi_ratio: f64,
    pub has_free_alternative: bool,
    pub best_free_quality: Option<f64>,
    pub scarcity_premium: f64,
    pub recommendation: String,
}

impl PaidDataEvaluator {
    /// Assess whether a paid dataset is worth purchasing.
    pub async fn evaluate(
        &self,
        metadata: &DatasetMetadata,
        quality: &QualityScore,
        free_alternatives: &[(&DatasetMetadata, &QualityScore)],
    ) -> Result<RoiReport> {
        let asking_price = metadata.price.amount;

        // Find best free alternative
        let best_free = free_alternatives
            .iter()
            .max_by(|a, b| a.1.total.partial_cmp(&b.1.total).unwrap());

        let has_free_alternative = best_free.is_some();
        let best_free_quality = best_free.map(|(_, q)| q.total);

        // Scarcity premium: fewer alternatives → higher value
        let scarcity_premium = if free_alternatives.is_empty() {
            1.5 // no alternatives, 50% premium
        } else {
            1.0 / (1.0 + free_alternatives.len() as f64 * 0.2)
        };

        // Quality premium over best free alternative
        let quality_delta = best_free_quality
            .map(|fq| (quality.total - fq).max(0.0))
            .unwrap_or(quality.total);

        let estimated_value = quality_delta * 0.01 * scarcity_premium;
        let roi_ratio = if asking_price > 0.0 {
            estimated_value / asking_price
        } else {
            f64::INFINITY
        };

        let recommendation = if roi_ratio > 2.0 {
            format!("Strong buy (ROI = {roi_ratio:.1}x)")
        } else if roi_ratio > 1.0 {
            format!("Buy (ROI = {roi_ratio:.1}x)")
        } else if has_free_alternative {
            format!("Skip — free alternative available (quality {:.0})", best_free_quality.unwrap_or(0.0))
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
            recommendation,
        })
    }
}
