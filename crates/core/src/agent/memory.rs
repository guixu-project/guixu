// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use super::contracts::HostKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemory {
    pub scope: MemoryScope,
    pub key: MemoryKey,
    pub profile: MemoryProfile,
    pub decisions: Vec<DecisionRecord>,
    pub concerns: Vec<Concern>,
    pub successful_mappings: Vec<DatasetMapping>,
    pub failed_attempts: Vec<FailedAttempt>,
    pub approvals: Vec<ApprovalRecord>,
    pub recent_segments: Vec<MemorySegment>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MemoryScope {
    Global,
    Workspace,
    TaskFamily,
    Job,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryKey {
    pub workspace_id: String,
    pub host_kind: HostKind,
    pub agent_id: Option<String>,
    pub task_family: Option<String>,
    pub job_id: Option<String>,
}

impl MemoryKey {
    pub fn global(host_kind: HostKind) -> Self {
        Self {
            workspace_id: "global".to_string(),
            host_kind,
            agent_id: None,
            task_family: None,
            job_id: None,
        }
    }

    pub fn workspace(workspace_id: &str, host_kind: HostKind) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            host_kind,
            agent_id: None,
            task_family: None,
            job_id: None,
        }
    }

    pub fn task_family(workspace_id: &str, host_kind: HostKind, task_family: &str) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            host_kind,
            agent_id: None,
            task_family: Some(task_family.to_string()),
            job_id: None,
        }
    }

    pub fn job(workspace_id: &str, host_kind: HostKind, job_id: &str) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            host_kind,
            agent_id: None,
            task_family: None,
            job_id: Some(job_id.to_string()),
        }
    }

    pub fn to_storage_key(&self) -> String {
        match (&self.task_family, &self.job_id) {
            (Some(tf), None) => format!(
                "mem:{}:{}:tf:{}",
                self.workspace_id,
                self.host_kind.as_str(),
                tf
            ),
            (None, Some(jid)) => format!(
                "mem:{}:{}:job:{}",
                self.workspace_id,
                self.host_kind.as_str(),
                jid
            ),
            (None, None) => format!("mem:{}:{}", self.workspace_id, self.host_kind.as_str()),
            _ => format!("mem:{}:{}", self.workspace_id, self.host_kind.as_str()),
        }
    }

    pub fn to_scope(&self) -> MemoryScope {
        if self.job_id.is_some() {
            MemoryScope::Job
        } else if self.task_family.is_some() {
            MemoryScope::TaskFamily
        } else if self.workspace_id == "global" {
            MemoryScope::Global
        } else {
            MemoryScope::Workspace
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryProfile {
    pub preferred_sources: Vec<String>,
    pub trusted_sources: Vec<String>,
    pub known_provider_quirks: HashMap<String, String>,
    pub default_budget_threshold_usd: Option<f64>,
    pub preferred_licenses: Vec<String>,
    pub evaluation_heuristics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub timestamp: DateTime<Utc>,
    pub job_id: String,
    pub dataset_cid: String,
    pub decision: Decision,
    pub reasoning: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Accepted,
    Rejected,
    NeedsApproval,
    Purchased,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concern {
    pub timestamp: DateTime<Utc>,
    pub severity: ConcernSeverity,
    pub topic: String,
    pub description: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConcernSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetMapping {
    pub task_description: String,
    pub dataset_cid: String,
    pub source: String,
    pub score: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedAttempt {
    pub timestamp: DateTime<Utc>,
    pub job_id: String,
    pub search_query: String,
    pub failure_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub timestamp: DateTime<Utc>,
    pub job_id: String,
    pub dataset_cid: String,
    pub approver: String,
    pub approved: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySegment {
    pub timestamp: DateTime<Utc>,
    pub segment_type: SegmentType,
    pub content: String,
    pub importance: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SegmentType {
    SearchResult,
    EvaluationNote,
    ApprovalNote,
    DownloadOutcome,
    UserFeedback,
}

/// A single memory mutation event, recorded as a trace span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMutation {
    /// What kind of mutation occurred.
    pub kind: MutationKind,
    /// Summary of the change.
    pub diff: MemoryDiff,
    /// The span that caused this mutation (if any).
    pub trigger_span_id: Option<String>,
}

/// Classification of memory mutation events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    MappingAdded,
    FailureRecorded,
    DecisionRecorded,
    ConcernAdded,
    ConcernResolved,
    SegmentAdded,
    TraceFeedback,
}

/// A lightweight diff describing what changed in memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDiff {
    /// Which field was modified (e.g. "successful_mappings", "concerns").
    pub field: String,
    /// Human-readable summary of the change.
    pub summary: String,
}

impl Default for AgentMemory {
    fn default() -> Self {
        Self {
            scope: MemoryScope::Global,
            key: MemoryKey::global(HostKind::openclaw()),
            profile: MemoryProfile::default(),
            decisions: Vec::new(),
            concerns: Vec::new(),
            successful_mappings: Vec::new(),
            failed_attempts: Vec::new(),
            approvals: Vec::new(),
            recent_segments: Vec::new(),
            updated_at: Utc::now(),
        }
    }
}

impl AgentMemory {
    pub fn record_decision(
        &mut self,
        job_id: &str,
        dataset_cid: &str,
        decision: Decision,
        reasoning: &str,
    ) {
        self.decisions.push(DecisionRecord {
            timestamp: Utc::now(),
            job_id: job_id.to_string(),
            dataset_cid: dataset_cid.to_string(),
            decision,
            reasoning: reasoning.to_string(),
        });
        self.updated_at = Utc::now();
    }

    pub fn record_successful_mapping(
        &mut self,
        task_description: &str,
        dataset_cid: &str,
        source: &str,
        score: f64,
    ) {
        self.successful_mappings.push(DatasetMapping {
            task_description: task_description.to_string(),
            dataset_cid: dataset_cid.to_string(),
            source: source.to_string(),
            score,
            timestamp: Utc::now(),
        });
        self.updated_at = Utc::now();
    }

    pub fn record_failed_attempt(
        &mut self,
        job_id: &str,
        search_query: &str,
        failure_reason: &str,
    ) {
        self.failed_attempts.push(FailedAttempt {
            timestamp: Utc::now(),
            job_id: job_id.to_string(),
            search_query: search_query.to_string(),
            failure_reason: failure_reason.to_string(),
        });
        self.updated_at = Utc::now();
    }

    pub fn add_concern(&mut self, severity: ConcernSeverity, topic: &str, description: &str) {
        self.concerns.push(Concern {
            timestamp: Utc::now(),
            severity,
            topic: topic.to_string(),
            description: description.to_string(),
            resolved: false,
        });
        self.updated_at = Utc::now();
    }

    pub fn resolve_concern(&mut self, topic: &str) {
        for concern in &mut self.concerns {
            if concern.topic == topic {
                concern.resolved = true;
            }
        }
        self.updated_at = Utc::now();
    }

    pub fn add_segment(&mut self, segment_type: SegmentType, content: &str, importance: f32) {
        self.recent_segments.push(MemorySegment {
            timestamp: Utc::now(),
            segment_type,
            content: content.to_string(),
            importance,
        });
        self.compact_segments();
        self.updated_at = Utc::now();
    }

    fn compact_segments(&mut self) {
        const MAX_SEGMENTS: usize = 100;
        if self.recent_segments.len() > MAX_SEGMENTS {
            self.recent_segments
                .sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            self.recent_segments.truncate(MAX_SEGMENTS);
        }
    }

    pub fn get_best_mapping(&self, task_description: &str) -> Option<&DatasetMapping> {
        self.successful_mappings
            .iter()
            .filter(|m| {
                m.task_description
                    .to_lowercase()
                    .contains(&task_description.to_lowercase())
            })
            .max_by(|a, b| {
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }
}
