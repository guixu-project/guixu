// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use data_agent::workflow::{WorkflowService, WorkflowState};
use data_storage::job_store::JobStore;

use data_storage::memory_store::MemoryStore;

use crate::state::AppState;

impl AppState {
    pub fn workflow_state_with_job_store(&self, job_store: JobStore) -> WorkflowState {
        let memory_store =
            MemoryStore::open(&data_core::config::NodeConfig::config_dir().join("memory_db"))
                .unwrap_or_else(|_| {
                    tracing::warn!("failed to open memory store, using temp dir");
                    MemoryStore::open(&std::env::temp_dir().join("guixu-memory-fallback"))
                        .expect("failed to open fallback memory store")
                });
        WorkflowState::new(
            self.store.clone(),
            self.feedback_store.clone(),
            self.search_engine.clone(),
            job_store,
            memory_store,
        )
    }

    pub fn workflow_service_with_job_store(&self, job_store: JobStore) -> WorkflowService {
        WorkflowService::new(self.workflow_state_with_job_store(job_store))
    }
}
