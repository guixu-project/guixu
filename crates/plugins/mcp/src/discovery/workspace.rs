// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{anyhow, Result};
use data_search::intent::QueryProfile;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::discovery::types::{
    ObservationRecord, WorkerProgress, WorkerRegistration, WorkerSnapshot, WorkerStatus,
    WorkspaceSnapshot,
};

pub struct WorkspaceManager {
    inner: RwLock<WorkspaceSnapshot>,
}

pub type WorkspaceHandle = Arc<WorkspaceManager>;

impl WorkspaceManager {
    pub fn new(raw_query: String, intent: QueryProfile) -> WorkspaceHandle {
        Arc::new(Self {
            inner: RwLock::new(WorkspaceSnapshot {
                workspace_id: format!("ws_{}", Uuid::new_v4()),
                raw_query,
                intent,
                workers: vec![],
                observations: vec![],
                all_results: vec![],
                errors: vec![],
            }),
        })
    }

    pub async fn register_worker(&self, registration: WorkerRegistration) -> Result<()> {
        let mut snapshot = self.inner.write().await;
        snapshot.workers.push(WorkerSnapshot {
            worker_id: registration.worker_id,
            skill_id: registration.skill_id,
            status: WorkerStatus::Running,
            progress: None,
            error: None,
        });
        Ok(())
    }

    pub async fn append_observation(&self, observation: ObservationRecord) -> Result<()> {
        let mut snapshot = self.inner.write().await;
        snapshot.all_results.push(observation.result.clone());
        snapshot.observations.push(observation);
        Ok(())
    }

    pub async fn update_worker_progress(
        &self,
        worker_id: &str,
        progress: WorkerProgress,
    ) -> Result<()> {
        let mut snapshot = self.inner.write().await;
        let worker = snapshot
            .workers
            .iter_mut()
            .find(|worker| worker.worker_id == worker_id)
            .ok_or_else(|| anyhow!("worker not found: {worker_id}"))?;
        worker.progress = Some(progress);
        Ok(())
    }

    pub async fn complete_worker(&self, worker_id: &str) -> Result<()> {
        let mut snapshot = self.inner.write().await;
        let worker = snapshot
            .workers
            .iter_mut()
            .find(|worker| worker.worker_id == worker_id)
            .ok_or_else(|| anyhow!("worker not found: {worker_id}"))?;
        worker.status = WorkerStatus::Completed;
        Ok(())
    }

    pub async fn fail_worker(&self, worker_id: &str, error: &str) -> Result<()> {
        let mut snapshot = self.inner.write().await;
        if let Some(worker) = snapshot
            .workers
            .iter_mut()
            .find(|worker| worker.worker_id == worker_id)
        {
            worker.status = WorkerStatus::Failed;
            worker.error = Some(error.to_string());
        }
        snapshot.errors.push(format!("{worker_id}: {error}"));
        Ok(())
    }

    pub async fn append_error(&self, error: &str) -> Result<()> {
        let mut snapshot = self.inner.write().await;
        snapshot.errors.push(error.to_string());
        Ok(())
    }

    pub async fn snapshot(&self) -> WorkspaceSnapshot {
        self.inner.read().await.clone()
    }

    pub async fn upsert_entity(&self, _key: &str, _payload: serde_json::Value) -> Result<()> {
        Ok(())
    }

    pub async fn record_conflict(
        &self,
        _key: &str,
        _field: &str,
        _values: serde_json::Value,
    ) -> Result<()> {
        Ok(())
    }

    pub async fn attach_valuation_signal(
        &self,
        _key: &str,
        _signal: serde_json::Value,
    ) -> Result<()> {
        Ok(())
    }

    pub async fn request_rebalance(
        &self,
        _worker_id: &str,
        _payload: serde_json::Value,
    ) -> Result<()> {
        Ok(())
    }

    pub async fn collect_memory_sync_candidates(&self) -> Result<Vec<serde_json::Value>> {
        Ok(vec![])
    }
}
