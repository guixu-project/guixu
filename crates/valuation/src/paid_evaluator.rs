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
    ///
    /// The estimated value uses an MMD (Maximum Mean Discrepancy) inspired
    /// distributional distance between the paid dataset and the best free
    /// alternative, replacing the former `quality_delta * scarcity * confidence`.
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

        // --- MMD-based estimated value ---
        // Approximate distributional distance between paid dataset and best free
        // alternative using a metadata-level MMD proxy.  We treat each quality
        // dimension as a feature and compute the squared distance in that space,
        // weighted by a Gaussian RBF kernel bandwidth σ² = 2·dim.
        let estimated_value = if let Some((free_meta, free_q)) = best_free {
            let mmd_sq = Self::metadata_mmd_squared(metadata, quality, free_meta, free_q);
            // Higher MMD → paid dataset is more different (and presumably better)
            // Scale to dollar-like value, modulated by community trust
            mmd_sq.sqrt() * 0.01 * scarcity_premium * (0.5 + community_confidence * 0.5)
        } else {
            // No free alternative — value is purely quality-driven
            quality.total * 0.01 * scarcity_premium * (0.5 + community_confidence * 0.5)
        };

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

    /// Metadata-level MMD² approximation.
    ///
    /// Computes squared Maximum Mean Discrepancy between two datasets using
    /// their quality score vectors as feature representations and a Gaussian
    /// RBF kernel.  This is a lightweight proxy for the full distributional
    /// MMD that would require raw data access.
    fn metadata_mmd_squared(
        a_meta: &DatasetMetadata,
        a_q: &QualityScore,
        b_meta: &DatasetMetadata,
        b_q: &QualityScore,
    ) -> f64 {
        // Quality-space distance
        let q_diffs = [
            a_q.completeness - b_q.completeness,
            a_q.consistency - b_q.consistency,
            a_q.freshness - b_q.freshness,
            a_q.schema_quality - b_q.schema_quality,
            a_q.provenance - b_q.provenance,
            a_q.community - b_q.community,
        ];
        let q_dist_sq: f64 = q_diffs.iter().map(|d| d * d).sum();

        // Schema-space distance: Jaccard distance on column names
        let cols_a: std::collections::HashSet<String> = a_meta
            .schema
            .columns
            .iter()
            .map(|c| c.name.to_lowercase())
            .collect();
        let cols_b: std::collections::HashSet<String> = b_meta
            .schema
            .columns
            .iter()
            .map(|c| c.name.to_lowercase())
            .collect();
        let intersection = cols_a.intersection(&cols_b).count() as f64;
        let union = cols_a.union(&cols_b).count() as f64;
        let jaccard_dist = if union > 0.0 { 1.0 - intersection / union } else { 1.0 };

        // RBF kernel: k(x,y) = exp(-||x-y||² / 2σ²), σ² = 2·dim
        let sigma_sq = 2.0 * q_diffs.len() as f64;
        let rbf_quality = (-q_dist_sq / (2.0 * sigma_sq)).exp();

        // MMD² ≈ 2(1 - kernel_mean)  (two-sample test with one sample each)
        // Blend quality-space and schema-space distances
        let kernel_mean = rbf_quality * 0.7 + (1.0 - jaccard_dist) * 0.3;
        (2.0 * (1.0 - kernel_mean)).max(0.0)
    }
}
