// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use data_search::engine::{RankedResult, SearchFilters};
use data_search::intent::QueryProfile;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct DiscoverySearchRequest {
    pub raw_query: String,
    pub filters: SearchFilters,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct DiscoverySearchOutput {
    pub intent: QueryProfile,
    pub results: Vec<RankedResult>,
    pub errors: Vec<String>,
    pub workspace: WorkspaceSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerRegistration {
    pub worker_id: String,
    pub skill_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerProgress {
    pub turn_index: usize,
    pub query_count: usize,
    pub result_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObservationRecord {
    pub worker_id: String,
    pub skill_id: String,
    pub query_variant: String,
    pub result: RankedResult,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceSnapshot {
    pub workspace_id: String,
    pub raw_query: String,
    pub intent: QueryProfile,
    pub workers: Vec<WorkerSnapshot>,
    pub observations: Vec<ObservationRecord>,
    pub all_results: Vec<RankedResult>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkerSnapshot {
    pub worker_id: String,
    pub skill_id: String,
    pub status: WorkerStatus,
    pub progress: Option<WorkerProgress>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceMetaOutput {
    pub workspace_id: String,
    pub worker_count: usize,
    pub observation_count: usize,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformSubAgentTurnInput {
    pub worker_id: String,
    pub skill_id: String,
    pub raw_query: String,
    pub intent: QueryProfile,
    pub current_result_count: usize,
    pub max_query_variants: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlatformSubAgentTurnOutput {
    #[serde(default)]
    pub query_variants: Vec<String>,
    #[serde(default)]
    pub finish: bool,
    #[serde(default)]
    pub rationale: String,
}
