// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use data_core::types::DatasetSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
            .filter(|rc| {
                dataset_cols
                    .iter()
                    .any(|dc| dc.contains(&rc.to_lowercase()))
            })
            .count();
        (matched as f64 / task.required_columns.len() as f64) * 100.0
    }

    fn compute_temporal_coverage(&self, metadata: &DatasetMetadata, task: &TaskContext) -> f64 {
        let Some((task_start, task_end)) = &task.time_range else {
            return 50.0; // No time range specified in task
        };

        let task_start_date = chrono::NaiveDate::parse_from_str(task_start, "%Y-%m-%d");
        let task_end_date = chrono::NaiveDate::parse_from_str(task_end, "%Y-%m-%d");

        match (task_start_date, task_end_date) {
            (Ok(ts), Ok(te)) => {
                let dataset_start = self.extract_temporal_start(metadata);
                let dataset_end = self.extract_temporal_end(metadata);

                // Compute overlap ratio
                let overlap_start = ts.max(dataset_start);
                let overlap_end = te.min(dataset_end);
                if overlap_end > overlap_start {
                    let overlap_days = (overlap_end - overlap_start).num_days() as f64;
                    let task_days = (te - ts).num_days().max(1) as f64;
                    (overlap_days / task_days * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0 // No temporal overlap
                }
            }
            _ => 50.0, // Could not parse dates
        }
    }

    fn extract_temporal_start(&self, metadata: &DatasetMetadata) -> chrono::NaiveDate {
        // Check tags for "temporal:YYYY-MM-DD" or "since:YYYY-MM-DD"
        for tag in &metadata.tags {
            if let Some(date_str) = tag
                .strip_prefix("temporal:")
                .or_else(|| tag.strip_prefix("since:"))
            {
                if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                    return date;
                }
            }
        }
        // Fall back to created_at
        metadata.created_at.date_naive()
    }

    fn extract_temporal_end(&self, metadata: &DatasetMetadata) -> chrono::NaiveDate {
        // Check tags for "until:YYYY-MM-DD" or "temporal_end:YYYY-MM-DD"
        for tag in &metadata.tags {
            if let Some(date_str) = tag
                .strip_prefix("until:")
                .or_else(|| tag.strip_prefix("temporal_end:"))
            {
                if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                    return date;
                }
            }
        }
        // Fall back to updated_at
        metadata.updated_at.date_naive()
    }

    /// PMI-based information gain estimate.
    ///
    /// Approximates Pointwise Mutual Information between the candidate dataset
    /// and the task by measuring how much the dataset's schema "co-occurs" with
    /// the task requirements beyond what random chance would predict.
    /// PMI(dataset, task) = log2( P(match) / (P(dataset_col) * P(task_col)) )
    ///
    /// This is preferred over raw KL divergence because PMI is incentive-
    /// compatible: data providers maximise their score by truthfully reporting
    /// their data (see "Truthful Dataset Valuation by PMI", 2024).
    fn compute_information_gain(&self, metadata: &DatasetMetadata, task: &TaskContext) -> f64 {
        if task.required_columns.is_empty() || metadata.schema.columns.is_empty() {
            return 50.0;
        }

        let dataset_cols: Vec<String> = metadata
            .schema
            .columns
            .iter()
            .map(|c| c.name.to_lowercase())
            .collect();
        let n_d = dataset_cols.len() as f64;
        let n_t = task.required_columns.len() as f64;
        let n_total = n_d + n_t; // universe size proxy

        let mut pmi_sum: f64 = 0.0;
        let mut matches: f64 = 0.0;

        for req in &task.required_columns {
            let req_lower = req.to_lowercase();
            let best = dataset_cols
                .iter()
                .map(|dc| {
                    if dc == &req_lower {
                        1.0
                    } else if dc.contains(&req_lower) || req_lower.contains(dc) {
                        0.5
                    } else {
                        0.0
                    }
                })
                .fold(0.0_f64, f64::max);

            if best > 0.0 {
                matches += best;
                // PMI = log2( P(co-occur) / (P(d) * P(t)) )
                let p_joint = best / n_total;
                let p_d = 1.0 / n_d;
                let p_t = 1.0 / n_t;
                pmi_sum += (p_joint / (p_d * p_t)).ln().max(0.0);
            }
        }

        // Normalise: scale PMI sum to [0, 100]
        let coverage = matches / n_t;
        let pmi_norm = if pmi_sum > 0.0 {
            (pmi_sum / n_t).min(1.0)
        } else {
            0.0
        };

        // Blend coverage (how many columns matched) with PMI (how surprising the match is)
        (coverage * 60.0 + pmi_norm * 40.0).clamp(0.0, 100.0)
    }

    fn compute_dedup_value(&self, metadata: &DatasetMetadata, task: &TaskContext) -> f64 {
        if task.existing_data_cids.is_empty() {
            100.0 // no existing data → maximum dedup value
        } else {
            let candidate_cols: HashSet<String> = metadata
                .schema
                .columns
                .iter()
                .map(|c| c.name.to_lowercase())
                .collect();

            let mut total_similarity = 0.0;
            let mut fetched = 0;

            // For each existing CID, try to fetch its schema and compute overlap
            for cid_str in &task.existing_data_cids {
                if let Some(existing_schema) = self.fetch_schema_by_cid(cid_str) {
                    let existing_cols: HashSet<String> = existing_schema
                        .columns
                        .iter()
                        .map(|c| c.name.to_lowercase())
                        .collect();

                    total_similarity += jaccard_similarity(&candidate_cols, &existing_cols);
                    fetched += 1;
                }
            }

            if fetched == 0 {
                return 70.0; // Could not fetch any existing schemas
            }

            // Higher overlap = lower dedup value (less new information)
            // Lower overlap = higher dedup value (more new columns)
            let avg_overlap = total_similarity / fetched as f64;
            (1.0 - avg_overlap) * 100.0
        }
    }

    /// Fetch schema by CID. In production, this would query DHT or local schema store.
    fn fetch_schema_by_cid(&self, _cid: &str) -> Option<DatasetSchema> {
        // TODO: In production, query DHT or local schema store
        None
    }
}

fn jaccard_similarity(set_a: &HashSet<String>, set_b: &HashSet<String>) -> f64 {
    let intersection = set_a.intersection(set_b).count() as f64;
    let union = set_a.union(set_b).count() as f64;
    if union == 0.0 {
        1.0
    } else {
        intersection / union
    }
}
