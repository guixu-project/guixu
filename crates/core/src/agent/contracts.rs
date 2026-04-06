// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use crate::types::{DataSource, DataType, DatasetCid, Price};

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
    pub allowed_sources: Vec<DataSource>,
    pub require_license_review: bool,
}

impl Default for TaskPolicy {
    fn default() -> Self {
        Self {
            allow_purchase: false,
            allowed_sources: vec![
                DataSource::Kaggle,
                DataSource::HuggingFace,
                DataSource::Ipfs,
                DataSource::GuixuHub,
            ],
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
    pub allowed_sources: Vec<DataSource>,
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
                allowed_sources: self.allowed_sources,
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
            allowed_sources: vec![DataSource::Kaggle, DataSource::HuggingFace],
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
}
