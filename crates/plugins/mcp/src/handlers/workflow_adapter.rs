// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use data_agent::workflow::{WorkflowService, WorkflowState};
use data_core::agent::contracts::DelegatedDataTask;
use data_storage::job_store::JobStore;

use crate::state::AppState;

impl AppState {
    pub fn workflow_state_with_job_store(&self, job_store: JobStore) -> WorkflowState {
        WorkflowState::new(
            self.store.clone(),
            self.feedback_store.clone(),
            self.search_engine.clone(),
            job_store,
        )
    }

    pub fn workflow_service_with_job_store(&self, job_store: JobStore) -> WorkflowService {
        WorkflowService::new(self.workflow_state_with_job_store(job_store))
    }
}
