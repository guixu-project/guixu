use data_core::feedback::CommunitySignal;
use data_core::metadata::DatasetMetadata;
use serde::{Deserialize, Serialize};

/// Task-Conditioned Value (TCV) engine.
///
/// TCV(D, T, C) = α·SchemaFit + β·TemporalFit + γ·InfoGain
///              + δ·Quality + ε·CommunitySignal - ζ·RiskPenalty
///
/// Range: [-100, +100]. Negative means the dataset would likely harm the task.
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
    /// TCV > 60: strongly recommended
    StrongPositive,
    /// TCV 30-60: recommended
    Positive,
    /// TCV 0-30: marginal value
    Neutral,
    /// TCV -30 to 0: likely unhelpful
    Negative,
    /// TCV < -30: would harm task performance
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
const BETA: f64 = 0.15;  // temporal fit
const GAMMA: f64 = 0.15; // information gain
const DELTA: f64 = 0.10; // quality
const EPSILON: f64 = 0.15; // community signal
const ZETA: f64 = 0.20;  // risk penalty (high weight — negative feedback matters)

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

        // TCV in range [-100, +100]
        let raw = ALPHA * schema_fit
            + BETA * temporal_fit
            + GAMMA * information_gain
            + DELTA * quality_score
            + EPSILON * community
            - ZETA * risk;
        let tcv_score = raw.clamp(-100.0, 100.0);

        let verdict = match tcv_score {
            s if s > 60.0 => TcvVerdict::StrongPositive,
            s if s > 30.0 => TcvVerdict::Positive,
            s if s > 0.0 => TcvVerdict::Neutral,
            s if s > -30.0 => TcvVerdict::Negative,
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
                dataset_cols.iter().any(|dc| dc.contains(&rc_lower) || rc_lower.contains(dc))
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

    fn compute_information_gain(&self, metadata: &DatasetMetadata, task: &TaskContext) -> f64 {
        if task.existing_data_cids.is_empty() {
            return 100.0; // no existing data → maximum gain
        }
        if task.existing_data_cids.contains(&metadata.cid.0) {
            return 0.0; // already have this exact dataset
        }
        // Heuristic: more columns = more potential new information
        let col_count = metadata.schema.columns.len() as f64;
        (col_count * 10.0).min(80.0) + 20.0 * (1.0 / task.existing_data_cids.len() as f64)
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

    fn explain(
        &self,
        verdict: &TcvVerdict,
        metadata: &DatasetMetadata,
        signal: &CommunitySignal,
        schema_fit: f64,
        community: f64,
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
