// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::agent::contracts::DelegatedDataTaskInput;
use serde_json::Value;

use super::trace_hooks::with_trace;
use crate::state::AppState;

pub async fn handle(args: Value, state: &AppState) -> Result<String> {
    with_trace(&state.trace_manager, "mcp.delegate", None, None, async {
        inner_handle(args, state).await
    })
    .await
}

async fn inner_handle(args: Value, state: &AppState) -> Result<String> {
    let input: DelegatedDataTaskInput = serde_json::from_value(args)?;
    let task = input.into_task();
    let job_id = task.job_id.clone();
    let goal = task.task.goal.clone();

    let workflow = state.workflow_service_with_job_store(state.job_store.clone());
    std::thread::spawn(move || {
        match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => {
                let _ = runtime.block_on(async move { workflow.run(task).await });
            }
            Err(error) => {
                tracing::error!(error = %error, "failed to build runtime for delegated workflow");
            }
        }
    });

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "job_id": job_id.to_string(),
        "status": "queued",
        "task_goal": goal,
        "message": "Job created. Use data_task_status to check progress."
    }))?)
}
