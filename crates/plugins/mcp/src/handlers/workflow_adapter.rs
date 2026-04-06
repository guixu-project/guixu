// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use data_agent::workflow::{WorkflowService, WorkflowState};
use data_core::agent::contracts::DelegatedDataTask;

use crate::state::AppState;

impl AppState {
    pub fn workflow_state(&self) -> WorkflowState {
        WorkflowState::new(
            self.store.clone(),
            self.feedback_store.clone(),
            self.search_engine.clone(),
        )
    }

    pub fn workflow_service(&self) -> WorkflowService {
        WorkflowService::new(self.workflow_state())
    }

    pub async fn run_workflow(
        &self,
        task: DelegatedDataTask,
    ) -> anyhow::Result<data_core::agent::contracts::JobResult> {
        let service = self.workflow_service();
        service.run(task).await
    }
}
