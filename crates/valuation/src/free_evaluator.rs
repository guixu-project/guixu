use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use serde::{Deserialize, Serialize};

/// Evaluates free datasets by Task Fitness — which free dataset best helps
/// the Agent complete its current task.
pub struct FreeDataEvaluator;

/// Task Fitness report for a free dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFitnessReport {
    pub total_score: f64,
    pub schema_relevance: f64,
    pub temporal_coverage: f64,
    pub information_gain: f64,
    pub data_quality: f64,
    pub freshness: f64,
    pub dedup_value: f64,
}

/// Agent's current task context for fitness evaluation.
#[derive(Debug, Clone)]
pub struct TaskContext {
    pub task_description: String,
    pub required_columns: Vec<String>,
    pub time_range: Option<(String, String)>,
    pub existing_data_cids: Vec<String>,
}

impl FreeDataEvaluator {
    /// Evaluate how well a free dataset fits the Agent's current task.
    pub async fn evaluate(
        &self,
        metadata: &DatasetMetadata,
        task: &TaskContext,
    ) -> Result<TaskFitnessReport> {
        let schema_relevance = self.compute_schema_relevance(metadata, task);
        let temporal_coverage = self.compute_temporal_coverage(metadata, task);
        let information_gain = self.compute_information_gain(metadata, task);
        let data_quality = metadata
            .stats
            .as_ref()
            .map(|s| (1.0 - s.null_rate) * 100.0)
            .unwrap_or(50.0);
        let freshness = {
            let age = (chrono::Utc::now() - metadata.updated_at).num_days() as f64;
            (100.0 - age * 0.5).max(0.0)
        };
        let dedup_value = self.compute_dedup_value(metadata, task);

        let total = schema_relevance * 0.30
            + temporal_coverage * 0.20
            + information_gain * 0.20
            + data_quality * 0.15
            + freshness * 0.10
            + dedup_value * 0.05;

        Ok(TaskFitnessReport {
            total_score: total,
            schema_relevance,
            temporal_coverage,
            information_gain,
            data_quality,
            freshness,
            dedup_value,
        })
    }

    fn compute_schema_relevance(&self, metadata: &DatasetMetadata, task: &TaskContext) -> f64 {
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
            .filter(|rc| dataset_cols.iter().any(|dc| dc.contains(&rc.to_lowercase())))
            .count();
        (matched as f64 / task.required_columns.len() as f64) * 100.0
    }

    fn compute_temporal_coverage(&self, _metadata: &DatasetMetadata, _task: &TaskContext) -> f64 {
        // TODO(milestone-3): parse temporal metadata and compare with task time_range
        50.0
    }

    fn compute_information_gain(&self, _metadata: &DatasetMetadata, _task: &TaskContext) -> f64 {
        // TODO(milestone-3): KL divergence between new data and Agent's existing data
        50.0
    }

    fn compute_dedup_value(&self, _metadata: &DatasetMetadata, task: &TaskContext) -> f64 {
        if task.existing_data_cids.is_empty() {
            100.0 // no existing data → maximum dedup value
        } else {
            // TODO(milestone-3): compare schema overlap with existing datasets
            70.0
        }
    }
}
