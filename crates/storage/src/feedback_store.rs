// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::feedback::{CommunitySignal, DatasetFeedback, TaskSignal, ValueAssessment};
use data_core::types::DatasetCid;
use rocksdb::DB;
use std::path::Path;
use std::sync::Arc;

/// Persistent store for dataset feedback (simulates on-chain EAS attestations).
/// In production, this would read from Base L2 EAS contract events.
#[derive(Clone)]
pub struct FeedbackStore {
    db: Arc<DB>,
}

impl FeedbackStore {
    pub fn open(path: &Path) -> Result<Self> {
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Record a feedback attestation.
    pub fn put(&self, feedback: &DatasetFeedback) -> Result<()> {
        let key = format!("fb:{}:{}", feedback.dataset_cid.0, feedback.id);
        let value = serde_json::to_vec(feedback)?;
        self.db.put(key.as_bytes(), &value)?;
        Ok(())
    }

    /// Get all feedback for a dataset.
    pub fn get_for_dataset(&self, cid: &DatasetCid) -> Result<Vec<DatasetFeedback>> {
        let prefix = format!("fb:{}:", cid.0);
        let mut results = vec![];
        let iter = self.db.prefix_iterator(prefix.as_bytes());
        for item in iter {
            let (k, v) = item?;
            if !k.starts_with(prefix.as_bytes()) {
                break;
            }
            if let Ok(fb) = serde_json::from_slice::<DatasetFeedback>(&v) {
                results.push(fb);
            }
        }
        Ok(results)
    }

    /// Aggregate feedback into a CommunitySignal.
    pub fn compute_signal(&self, cid: &DatasetCid) -> Result<CommunitySignal> {
        let feedbacks = self.get_for_dataset(cid)?;
        if feedbacks.is_empty() {
            return Ok(CommunitySignal {
                dataset_cid: cid.clone(),
                total_reviews: 0,
                avg_relevance: 0.0,
                avg_quality: 0.0,
                positive_rate: 0.0,
                negative_rate: 0.0,
                task_signals: vec![],
            });
        }

        let n = feedbacks.len() as f64;
        let avg_relevance = feedbacks.iter().map(|f| f.relevance_score).sum::<f64>() / n;
        let avg_quality = feedbacks
            .iter()
            .map(|f| f.quality_rating as f64)
            .sum::<f64>()
            / n;
        let positive_count = feedbacks
            .iter()
            .filter(|f| f.value_assessment == ValueAssessment::Positive)
            .count();
        let negative_count = feedbacks
            .iter()
            .filter(|f| f.value_assessment == ValueAssessment::Negative)
            .count();

        // Group by task_type
        let mut task_map: std::collections::HashMap<String, Vec<&DatasetFeedback>> =
            std::collections::HashMap::new();
        for fb in &feedbacks {
            task_map.entry(fb.task_type.clone()).or_default().push(fb);
        }

        let task_signals: Vec<TaskSignal> = task_map
            .into_iter()
            .map(|(task_type, fbs)| {
                let count = fbs.len() as u64;
                let avg_rel = fbs.iter().map(|f| f.relevance_score).sum::<f64>() / count as f64;
                let success_rate =
                    fbs.iter().filter(|f| f.task_success).count() as f64 / count as f64;
                TaskSignal {
                    task_type,
                    count,
                    avg_relevance: avg_rel,
                    success_rate,
                }
            })
            .collect();

        Ok(CommunitySignal {
            dataset_cid: cid.clone(),
            total_reviews: feedbacks.len() as u64,
            avg_relevance,
            avg_quality,
            positive_rate: positive_count as f64 / n,
            negative_rate: negative_count as f64 / n,
            task_signals,
        })
    }
}
