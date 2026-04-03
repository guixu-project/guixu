// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{DatasetCid, Did};

/// On-chain feedback attestation — records how an agent used a dataset
/// and whether it helped or harmed the task. Analogous to product reviews.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetFeedback {
    pub id: String,
    pub dataset_cid: DatasetCid,
    pub agent_did: Did,
    pub task_type: String,
    pub task_description: String,
    /// -1.0 (actively harmful) to 1.0 (perfectly relevant)
    pub relevance_score: f64,
    /// 1-5 star rating
    pub quality_rating: u8,
    pub task_success: bool,
    pub value_assessment: ValueAssessment,
    pub comment: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueAssessment {
    /// Dataset contributed positively to task completion
    Positive,
    /// Dataset had no meaningful impact
    Neutral,
    /// Dataset degraded task performance (noise, bias, wrong schema, etc.)
    Negative,
}

/// Aggregated community signal for a dataset, computed from all feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunitySignal {
    pub dataset_cid: DatasetCid,
    pub total_reviews: u64,
    pub avg_relevance: f64,
    pub avg_quality: f64,
    pub positive_rate: f64,
    pub negative_rate: f64,
    /// Per-task-type breakdown of feedback
    pub task_signals: Vec<TaskSignal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSignal {
    pub task_type: String,
    pub count: u64,
    pub avg_relevance: f64,
    pub success_rate: f64,
}

impl CommunitySignal {
    /// Compute a score (0-100) for a specific task type.
    /// Returns 50.0 (neutral) if no relevant feedback exists.
    pub fn score_for_task(&self, task_type: &str) -> f64 {
        // Find task-specific signal
        if let Some(ts) = self.task_signals.iter().find(|t| t.task_type == task_type) {
            if ts.count == 0 {
                return 50.0;
            }
            // Weighted: relevance (0-1 → 0-100) * confidence from count
            let confidence = 1.0 - (1.0 / (1.0 + ts.count as f64 * 0.2));
            let base = (ts.avg_relevance * 50.0 + 50.0) * ts.success_rate;
            // Blend with neutral based on confidence
            50.0 * (1.0 - confidence) + base * confidence
        } else if self.total_reviews > 0 {
            // Fall back to global signal with lower confidence
            let confidence = 1.0 - (1.0 / (1.0 + self.total_reviews as f64 * 0.1));
            let base = (self.avg_relevance * 50.0 + 50.0) * (1.0 - self.negative_rate);
            50.0 * (1.0 - confidence) + base * confidence
        } else {
            50.0
        }
    }

    /// Compute risk penalty (0-100) based on negative feedback.
    pub fn risk_penalty(&self) -> f64 {
        if self.total_reviews == 0 {
            return 0.0;
        }
        // High negative rate → high penalty
        self.negative_rate * 100.0
    }
}
