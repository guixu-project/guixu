// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::feedback::{CommunitySignal, DatasetFeedback, TaskSignal, ValueAssessment};
use data_core::types::DatasetCid;
use rocksdb::DB;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Persistent store for dataset feedback (simulates on-chain EAS attestations).
/// In production, this would read from Base L2 EAS contract events.
#[derive(Clone)]
pub struct FeedbackStore {
    db: Arc<DB>,
    feedback_cache: Arc<RwLock<HashMap<String, Vec<DatasetFeedback>>>>,
    signal_cache: Arc<RwLock<HashMap<String, CommunitySignal>>>,
}

impl FeedbackStore {
    pub fn open(path: &Path) -> Result<Self> {
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)?;
        let db = Arc::new(db);
        let feedback_cache = Arc::new(RwLock::new(load_feedback_cache(&db)?));
        Ok(Self {
            db,
            feedback_cache,
            signal_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Record a feedback attestation.
    pub fn put(&self, feedback: &DatasetFeedback) -> Result<()> {
        let key = format!("fb:{}:{}", feedback.dataset_cid.0, feedback.id);
        let value = serde_json::to_vec(feedback)?;
        self.db.put(key.as_bytes(), &value)?;
        {
            let mut cache = self.feedback_cache.write().unwrap();
            let items = cache.entry(feedback.dataset_cid.0.clone()).or_default();
            if let Some(existing) = items.iter_mut().find(|item| item.id == feedback.id) {
                *existing = feedback.clone();
            } else {
                items.push(feedback.clone());
            }
        }
        self.signal_cache
            .write()
            .unwrap()
            .remove(&feedback.dataset_cid.0);
        Ok(())
    }

    /// Get all feedback for a dataset.
    pub fn get_for_dataset(&self, cid: &DatasetCid) -> Result<Vec<DatasetFeedback>> {
        Ok(self
            .feedback_cache
            .read()
            .unwrap()
            .get(&cid.0)
            .cloned()
            .unwrap_or_default())
    }

    /// Aggregate feedback into a CommunitySignal.
    pub fn compute_signal(&self, cid: &DatasetCid) -> Result<CommunitySignal> {
        if let Some(signal) = self.signal_cache.read().unwrap().get(&cid.0).cloned() {
            return Ok(signal);
        }

        let signal = {
            let cache = self.feedback_cache.read().unwrap();
            aggregate_signal(cid, cache.get(&cid.0).map(Vec::as_slice).unwrap_or(&[]))
        };
        self.signal_cache
            .write()
            .unwrap()
            .insert(cid.0.clone(), signal.clone());
        Ok(signal)
    }
}

fn load_feedback_cache(db: &DB) -> Result<HashMap<String, Vec<DatasetFeedback>>> {
    let mut feedback_by_cid = HashMap::new();
    let iter = db.prefix_iterator(b"fb:");
    for item in iter {
        let (key, value) = item?;
        if !key.starts_with(b"fb:") {
            break;
        }
        if let Ok(feedback) = serde_json::from_slice::<DatasetFeedback>(&value) {
            feedback_by_cid
                .entry(feedback.dataset_cid.0.clone())
                .or_insert_with(Vec::new)
                .push(feedback);
        }
    }
    Ok(feedback_by_cid)
}

fn aggregate_signal(cid: &DatasetCid, feedbacks: &[DatasetFeedback]) -> CommunitySignal {
    if feedbacks.is_empty() {
        return CommunitySignal {
            dataset_cid: cid.clone(),
            total_reviews: 0,
            avg_relevance: 0.0,
            avg_quality: 0.0,
            positive_rate: 0.0,
            negative_rate: 0.0,
            task_signals: vec![],
        };
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

    let mut task_map: HashMap<String, Vec<&DatasetFeedback>> = HashMap::new();
    for feedback in feedbacks {
        task_map
            .entry(feedback.task_type.clone())
            .or_default()
            .push(feedback);
    }

    let task_signals: Vec<TaskSignal> = task_map
        .into_iter()
        .map(|(task_type, items)| {
            let count = items.len() as u64;
            let avg_rel = items.iter().map(|f| f.relevance_score).sum::<f64>() / count as f64;
            let success_rate =
                items.iter().filter(|f| f.task_success).count() as f64 / count as f64;
            TaskSignal {
                task_type,
                count,
                avg_relevance: avg_rel,
                success_rate,
            }
        })
        .collect();

    CommunitySignal {
        dataset_cid: cid.clone(),
        total_reviews: feedbacks.len() as u64,
        avg_relevance,
        avg_quality,
        positive_rate: positive_count as f64 / n,
        negative_rate: negative_count as f64 / n,
        task_signals,
    }
}
