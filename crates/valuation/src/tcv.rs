use data_core::feedback::CommunitySignal;
use data_core::metadata::DatasetMetadata;
use data_core::types::DataType;
use serde::{Deserialize, Serialize};

use crate::video_evaluator::VideoEvaluator;

/// Task-Conditioned Value (TCV) engine.
///
/// TCV(D, T, C) = α·SchemaFit + β·TemporalFit + γ·InfoGain
///              + δ·Quality + ε·CommunitySignal - ζ·RiskPenalty
///
/// Range: [0, 100]. Scores around 50 are neutral after normalization.
pub struct TcvEngine;

/// Full TCV valuation report returned to the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcvReport {
    pub tcv_score: f64,
    pub schema_fit: f64,
    pub temporal_fit: f64,
    pub information_gain: f64,
    pub quality_score: f64,
    pub community_signal: f64,
    pub risk_penalty: f64,
    pub verdict: TcvVerdict,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TcvVerdict {
    /// TCV > 80: strongly recommended
    StrongPositive,
    /// TCV 65-80: recommended
    Positive,
    /// TCV 50-65: marginal value
    Neutral,
    /// TCV 35-50: likely unhelpful
    Negative,
    /// TCV <= 35: would likely harm task performance
    StrongNegative,
}

/// Agent's task context for TCV computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub task_description: String,
    pub task_type: String,
    pub required_columns: Vec<String>,
    pub time_range: Option<(String, String)>,
    pub existing_data_cids: Vec<String>,
    pub budget: f64,
}

const ALPHA: f64 = 0.25; // schema fit
const BETA: f64 = 0.15; // temporal fit
const GAMMA: f64 = 0.15; // information gain
const DELTA: f64 = 0.10; // quality
const EPSILON: f64 = 0.15; // community signal
const ZETA: f64 = 0.20; // risk penalty (high weight — negative feedback matters)

impl TcvEngine {
    /// Compute Task-Conditioned Value for a dataset.
    pub fn evaluate(
        &self,
        metadata: &DatasetMetadata,
        task: &TaskContext,
        signal: &CommunitySignal,
    ) -> TcvReport {
        let schema_fit = self.compute_schema_fit(metadata, task);
        let temporal_fit = self.compute_temporal_fit(metadata, task);
        let information_gain = self.compute_information_gain(metadata, task);
        let quality_score = self.compute_quality(metadata);
        let community = signal.score_for_task(&task.task_type);
        let risk = signal.risk_penalty();

        // Type-aware bonus: blend domain-specific score into quality dimension
        let type_bonus = match metadata.data_type {
            DataType::Video => {
                let vr = VideoEvaluator::evaluate(metadata);
                // Video quality replaces half of generic quality
                vr.total * 0.5
            }
            _ => 0.0,
        };
        let quality_score =
            quality_score * (1.0 - if type_bonus > 0.0 { 0.5 } else { 0.0 }) + type_bonus;

        // Compute the legacy centered score, then normalize it into [0, 100]
        // so downstream consumers never see negative TCV values.
        let raw = ALPHA * schema_fit
            + BETA * temporal_fit
            + GAMMA * information_gain
            + DELTA * quality_score
            + EPSILON * community
            - ZETA * risk;
        let centered_score = raw.clamp(-100.0, 100.0);
        let tcv_score = ((centered_score + 100.0) / 2.0).clamp(0.0, 100.0);

        let verdict = match tcv_score {
            s if s > 80.0 => TcvVerdict::StrongPositive,
            s if s > 65.0 => TcvVerdict::Positive,
            s if s > 50.0 => TcvVerdict::Neutral,
            s if s > 35.0 => TcvVerdict::Negative,
            _ => TcvVerdict::StrongNegative,
        };

        let explanation = self.explain(
            &verdict, metadata, signal, schema_fit, community, risk, tcv_score,
        );

        TcvReport {
            tcv_score,
            schema_fit,
            temporal_fit,
            information_gain,
            quality_score,
            community_signal: community,
            risk_penalty: risk,
            verdict,
            explanation,
        }
    }

    fn compute_schema_fit(&self, metadata: &DatasetMetadata, task: &TaskContext) -> f64 {
        if task.required_columns.is_empty() {
            return 50.0;
        }
        let dataset_cols: Vec<String> = metadata
            .schema
            .columns
            .iter()
            .map(|c| c.name.to_lowercase())
            .collect();
        let matched = task
            .required_columns
            .iter()
            .filter(|rc| {
                let rc_lower = rc.to_lowercase();
                dataset_cols
                    .iter()
                    .any(|dc| dc.contains(&rc_lower) || rc_lower.contains(dc))
            })
            .count();
        (matched as f64 / task.required_columns.len() as f64) * 100.0
    }

    fn compute_temporal_fit(&self, metadata: &DatasetMetadata, task: &TaskContext) -> f64 {
        match &task.time_range {
            Some((start, end)) => {
                // Check if dataset tags or description mention the time range
                let all_text = format!(
                    "{} {} {}",
                    metadata.title,
                    metadata.description.as_deref().unwrap_or(""),
                    metadata.tags.join(" ")
                )
                .to_lowercase();
                let has_start = all_text.contains(&start.to_lowercase());
                let has_end = all_text.contains(&end.to_lowercase());
                match (has_start, has_end) {
                    (true, true) => 100.0,
                    (true, false) | (false, true) => 60.0,
                    (false, false) => 30.0,
                }
            }
            None => 50.0,
        }
    }

    /// KNN-Shapley–inspired information gain.
    ///
    /// Treats each column as a "player" in a cooperative game and approximates
    /// its marginal contribution via a leave-one-out style calculation over the
    /// task's required columns (the "neighbours").  This replaces the former
    /// column-count heuristic with a theoretically grounded Shapley proxy.
    fn compute_information_gain(&self, metadata: &DatasetMetadata, task: &TaskContext) -> f64 {
        if task.existing_data_cids.is_empty() {
            return 100.0; // no existing data → maximum gain
        }
        if task.existing_data_cids.contains(&metadata.cid.0) {
            return 0.0; // already have this exact dataset
        }

        let dataset_cols: Vec<String> = metadata
            .schema
            .columns
            .iter()
            .map(|c| c.name.to_lowercase())
            .collect();

        if task.required_columns.is_empty() {
            // No task columns specified — fall back to novelty discount
            let novelty = 1.0 / (1.0 + task.existing_data_cids.len() as f64);
            return (novelty * 100.0).min(100.0);
        }

        // Marginal contribution of each dataset column w.r.t. required columns
        // (KNN-Shapley approximation: value ∝ 1/rank of match quality)
        let mut shapley_sum: f64 = 0.0;

        for (rank, req) in task.required_columns.iter().enumerate() {
            let req_lower = req.to_lowercase();
            // Best match score for this required column among dataset columns
            let best_match = dataset_cols
                .iter()
                .map(|dc| {
                    if dc == &req_lower {
                        1.0
                    } else if dc.contains(&req_lower) || req_lower.contains(dc) {
                        0.6
                    } else {
                        0.0
                    }
                })
                .fold(0.0_f64, f64::max);

            // Shapley weighting: earlier (higher-priority) columns matter more
            let weight = 1.0 / (1.0 + rank as f64);
            shapley_sum += best_match * weight;
        }

        // Normalise by harmonic number H_n (sum of weights)
        let harmonic_n: f64 = (1..=task.required_columns.len())
            .map(|k| 1.0 / k as f64)
            .sum();
        let base_score = (shapley_sum / harmonic_n) * 100.0;

        // Discount by existing data redundancy
        let redundancy_discount = 1.0 / (1.0 + task.existing_data_cids.len() as f64 * 0.3);
        (base_score * redundancy_discount).clamp(0.0, 100.0)
    }

    fn compute_quality(&self, metadata: &DatasetMetadata) -> f64 {
        let completeness = metadata
            .stats
            .as_ref()
            .map(|s| (1.0 - s.null_rate) * 100.0)
            .unwrap_or(50.0);
        let freshness = {
            let age = (chrono::Utc::now() - metadata.updated_at).num_days() as f64;
            (100.0 - age * 0.5).max(0.0)
        };
        let has_schema = (!metadata.schema.columns.is_empty()) as u8 as f64 * 100.0;
        completeness * 0.4 + freshness * 0.3 + has_schema * 0.3
    }

    #[allow(clippy::too_many_arguments)]
    fn explain(
        &self,
        verdict: &TcvVerdict,
        _metadata: &DatasetMetadata,
        signal: &CommunitySignal,
        schema_fit: f64,
        _community: f64,
        risk: f64,
        tcv: f64,
    ) -> String {
        let mut parts = vec![format!("TCV={tcv:.1}")];

        if schema_fit > 70.0 {
            parts.push("schema matches task requirements well".into());
        } else if schema_fit < 30.0 {
            parts.push("schema poorly matches task requirements".into());
        }

        if signal.total_reviews > 0 {
            parts.push(format!(
                "{} previous reviews ({:.0}% positive)",
                signal.total_reviews,
                signal.positive_rate * 100.0
            ));
        }

        if risk > 20.0 {
            parts.push(format!(
                "⚠️ {:.0}% negative feedback rate — may harm task",
                signal.negative_rate * 100.0
            ));
        }

        match verdict {
            TcvVerdict::StrongPositive => parts.push("strongly recommended".into()),
            TcvVerdict::Positive => parts.push("recommended".into()),
            TcvVerdict::Neutral => parts.push("marginal value".into()),
            TcvVerdict::Negative => parts.push("likely unhelpful for this task".into()),
            TcvVerdict::StrongNegative => {
                parts.push("⚠️ NEGATIVE VALUE: would likely harm task performance".into())
            }
        }

        parts.join(". ")
    }
}
