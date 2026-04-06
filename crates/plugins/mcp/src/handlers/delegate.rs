// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::agent::contracts::DelegatedDataTaskInput;
use serde_json::Value;

use crate::state::AppState;

pub async fn handle(args: Value, _state: &AppState) -> Result<String> {
    let input: DelegatedDataTaskInput = serde_json::from_value(args)?;
    let task = input.into_task();
    let job_id = task.job_id.clone();

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "job_id": job_id.to_string(),
        "status": "queued",
        "task_goal": task.task.goal,
        "message": "Job created. Use data_task_status to check progress."
    }))?)
}
