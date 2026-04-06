// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use crate::types::{
    DataSource, DataType, DatasetCid, Price, SkillCapability, SourceFamily,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegatedDataTask {
    pub job_id: JobId,
    pub host: HostContext,
    pub workspace: WorkspaceContext,
    pub task: DataTaskSpec,
    pub policy: TaskPolicy,
    pub desired_outputs: Vec<OutputKind>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostContext {
    pub kind: HostKind,
    pub session_key: String,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HostKind {
    OpenClaw,
    Codex,
    OpenCode,
}

impl HostKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            HostKind::OpenClaw => "openclaw",
            HostKind::Codex => "codex",
            HostKind::OpenCode => "opencode",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceContext {
    pub id: String,
    pub root_hint: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTaskSpec {
    pub goal: String,
    pub task_type: Option<String>,
    pub required_modalities: Vec<DataType>,
    pub required_columns: Vec<String>,
    pub budget: Option<Budget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    pub amount: f64,
    pub currency: String,
}

impl Budget {
    pub fn usd(amount: f64) -> Self {
        Self {
            amount,
            currency: "USD".into(),
        }
    }

    pub fn as_price(&self) -> Price {
        Price {
            amount: self.amount,
            currency: self.currency.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPolicy {
    pub allow_purchase: bool,
    #[serde(default, alias = "allowed_sources")]
    pub allowed_skill_ids: Vec<String>,
    #[serde(default)]
    pub blocked_skill_ids: Vec<String>,
    #[serde(default)]
    pub allowed_source_families: Vec<SourceFamily>,
    #[serde(default)]
    pub required_capabilities: Vec<SkillCapability>,
    pub require_license_review: bool,
}

impl Default for TaskPolicy {
    fn default() -> Self {
        Self {
            allow_purchase: false,
            allowed_skill_ids: vec![
                "kaggle".into(),
                "huggingface".into(),
                "ipfs".into(),
                "guixu_hub".into(),
            ],
            blocked_skill_ids: vec![],
            allowed_source_families: vec![],
            required_capabilities: vec![SkillCapability::Search],
            require_license_review: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputKind {
    SelectedDataset,
    EvaluationReport,
    DownloadedArtifact,
    GuixuLock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegatedDataTaskInput {
    pub host_kind: HostKind,
    pub session_key: String,
    pub run_id: Option<String>,
    pub workspace_id: String,
    pub workspace_root: Option<PathBuf>,
    pub goal: String,
    pub task_type: Option<String>,
    pub required_modalities: Vec<DataType>,
    pub required_columns: Vec<String>,
    pub budget: Option<Budget>,
    pub allow_purchase: bool,
    #[serde(default, alias = "allowed_sources")]
    pub allowed_skill_ids: Vec<String>,
    #[serde(default)]
    pub blocked_skill_ids: Vec<String>,
    #[serde(default)]
    pub allowed_source_families: Vec<SourceFamily>,
    #[serde(default)]
    pub required_capabilities: Vec<SkillCapability>,
    pub require_license_review: bool,
    pub desired_outputs: Vec<OutputKind>,
}

impl DelegatedDataTaskInput {
    pub fn into_task(self) -> DelegatedDataTask {
        DelegatedDataTask {
            job_id: JobId::new(),
            host: HostContext {
                kind: self.host_kind,
                session_key: self.session_key,
                run_id: self.run_id,
            },
            workspace: WorkspaceContext {
                id: self.workspace_id,
                root_hint: self.workspace_root,
            },
            task: DataTaskSpec {
                goal: self.goal,
                task_type: self.task_type,
                required_modalities: self.required_modalities,
                required_columns: self.required_columns,
                budget: self.budget,
            },
            policy: TaskPolicy {
                allow_purchase: self.allow_purchase,
                allowed_skill_ids: self.allowed_skill_ids,
                blocked_skill_ids: self.blocked_skill_ids,
                allowed_source_families: self.allowed_source_families,
                required_capabilities: self.required_capabilities,
                require_license_review: self.require_license_review,
            },
            desired_outputs: self.desired_outputs,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct JobId(pub String);

impl JobId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "job_{}", self.0)
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatus {
    pub job_id: JobId,
    pub state: JobState,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTask {
    pub task_id: String,
    pub parent_job_id: JobId,
    pub kind: WorkerTaskKind,
    pub label: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerTaskKind {
    SearchSkill,
    EvaluateCandidate,
    LicenseReview,
    FetchCommunitySignal,
    DownloadArtifact,
    BuildArtifact,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerTaskState {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTaskResult {
    pub task_id: String,
    pub parent_job_id: JobId,
    pub kind: WorkerTaskKind,
    pub status: WorkerTaskState,
    pub payload: serde_json::Value,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobEvent {
    pub event_id: String,
    pub job_id: JobId,
    pub event_type: JobEventType,
    pub message: String,
    pub worker_task_id: Option<String>,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobEventType {
    JobQueued,
    JobStarted,
    JobCompleted,
    JobFailed,
    ApprovalRequired,
    WorkerStarted,
    WorkerCompleted,
    WorkerFailed,
    ArtifactProduced,
}

impl WorkerTask {
    pub fn new(parent_job_id: JobId, kind: WorkerTaskKind, label: impl Into<String>) -> Self {
        Self {
            task_id: uuid::Uuid::new_v4().to_string(),
            parent_job_id,
            kind,
            label: label.into(),
            created_at: Utc::now(),
        }
    }
}

impl JobEvent {
    pub fn new(
        job_id: JobId,
        event_type: JobEventType,
        message: impl Into<String>,
        worker_task_id: Option<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            job_id,
            event_type,
            message: message.into(),
            worker_task_id,
            payload,
            timestamp: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    AwaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub job_id: JobId,
    pub selected_dataset: Option<DatasetCid>,
    pub artifacts: Vec<ArtifactRef>,
    pub memory_updates: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub kind: OutputKind,
    pub uri: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delegated_data_task_input_into_task() {
        let input = DelegatedDataTaskInput {
            host_kind: HostKind::OpenClaw,
            session_key: "agent:main:main".into(),
            run_id: Some("run_123".into()),
            workspace_id: "repo:guixu-demo".into(),
            workspace_root: Some(PathBuf::from("/workspace/project")),
            goal: "train a safety helmet detector".into(),
            task_type: Some("detection".into()),
            required_modalities: vec![DataType::Image],
            required_columns: vec!["image_path".into(), "bbox".into()],
            budget: Some(Budget::usd(20.0)),
            allow_purchase: false,
            allowed_skill_ids: vec!["kaggle".into(), "huggingface".into()],
            blocked_skill_ids: vec![],
            allowed_source_families: vec![],
            required_capabilities: vec![SkillCapability::Search],
            require_license_review: true,
            desired_outputs: vec![OutputKind::SelectedDataset],
        };

        let task = input.into_task();
        assert_eq!(task.host.kind, HostKind::OpenClaw);
        assert_eq!(task.workspace.id, "repo:guixu-demo");
        assert_eq!(task.task.goal, "train a safety helmet detector");
        assert!(task.policy.allow_purchase == false);
    }

    #[test]
    fn test_job_id_display() {
        let job_id = JobId::new();
        let display = job_id.to_string();
        assert!(display.starts_with("job_"));
    }

    #[test]
    fn test_budget_as_price() {
        let budget = Budget::usd(25.0);
        let price = budget.as_price();
        assert_eq!(price.amount, 25.0);
        assert_eq!(price.currency, "USD");
    }

    #[test]
    fn test_worker_task_and_job_event_creation() {
        let job_id = JobId::new();
        let worker = WorkerTask::new(
            job_id.clone(),
            WorkerTaskKind::SearchSkill,
            "search huggingface",
        );
        assert_eq!(worker.parent_job_id, job_id);
        assert_eq!(worker.kind, WorkerTaskKind::SearchSkill);
        assert_eq!(worker.label, "search huggingface");

        let event = JobEvent::new(
            worker.parent_job_id.clone(),
            JobEventType::WorkerStarted,
            "worker started",
            Some(worker.task_id.clone()),
            serde_json::json!({ "skill_id": "huggingface" }),
        );
        assert_eq!(event.event_type, JobEventType::WorkerStarted);
        assert_eq!(
            event.worker_task_id.as_deref(),
            Some(worker.task_id.as_str())
        );
    }
}
