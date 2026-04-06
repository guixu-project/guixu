// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::agent::contracts::{JobEvent, JobEventType, JobId, JobState};
use serde_json::json;
use serde_json::Value;

use crate::state::AppState;

fn parse_job_id(raw: &str) -> JobId {
    if let Some(stripped) = raw.strip_prefix("job_") {
        JobId(stripped.to_string())
    } else {
        JobId(raw.to_string())
    }
}

pub async fn status(args: Value, state: &AppState) -> Result<String> {
    let job_id = args
        .get("job_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing job_id"))?;
    let job_id = parse_job_id(job_id);
    let status = state.job_store.get_status(&job_id)?;
    let result = state.job_store.get_result(&job_id)?;
    let events = state.job_store.list_events(&job_id)?;

    Ok(serde_json::to_string_pretty(&json!({
        "job_id": job_id.to_string(),
        "status": status.as_ref().map(|s| s.state),
        "updated_at": status.as_ref().map(|s| s.updated_at),
        "selected_dataset": result.as_ref().and_then(|r| r.selected_dataset.as_ref().map(|cid| cid.0.clone())),
        "artifacts": result.as_ref().map(|r| r.artifacts.clone()).unwrap_or_default(),
        "errors": result.as_ref().map(|r| r.errors.clone()).unwrap_or_default(),
        "events": events,
    }))?)
}

pub async fn approve(args: Value, state: &AppState) -> Result<String> {
    let job_id = args
        .get("job_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing job_id"))?;
    let job_id = parse_job_id(job_id);
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing action"))?;
    let approved = args
        .get("approved")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let notes = args.get("notes").and_then(|v| v.as_str());

    state.job_store.append_event(&JobEvent::new(
        job_id.clone(),
        JobEventType::ApprovalRequired,
        format!("approval decision recorded for action: {action}"),
        None,
        json!({
            "action": action,
            "approved": approved,
            "notes": notes,
        }),
    ))?;

    Ok(serde_json::to_string_pretty(&json!({
        "job_id": job_id.to_string(),
        "action": action,
        "decision": if approved { "approved" } else { "rejected" },
        "notes": notes,
        "message": "Approval decision recorded. Purchase/publish integration is still pending."
    }))?)
}

pub async fn cancel(args: Value, state: &AppState) -> Result<String> {
    let job_id = args
        .get("job_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing job_id"))?;
    let job_id = parse_job_id(job_id);
    let reason = args.get("reason").and_then(|v| v.as_str());

    let _ = state.job_store.update_state(&job_id, JobState::Cancelled);
    state.job_store.append_event(&JobEvent::new(
        job_id.clone(),
        JobEventType::JobFailed,
        "job cancelled",
        None,
        json!({ "reason": reason, "cancelled": true }),
    ))?;

    Ok(serde_json::to_string_pretty(&json!({
        "job_id": job_id.to_string(),
        "status": "cancelled",
        "reason": reason,
        "message": "Job cancelled."
    }))?)
}

pub async fn artifacts(args: Value, state: &AppState) -> Result<String> {
    let job_id = args
        .get("job_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing job_id"))?;
    let job_id = parse_job_id(job_id);
    let result = state.job_store.get_result(&job_id)?;

    Ok(serde_json::to_string_pretty(&json!({
        "job_id": job_id.to_string(),
        "artifacts": result.as_ref().map(|r| r.artifacts.clone()).unwrap_or_default(),
        "selected_dataset": result.and_then(|r| r.selected_dataset.map(|cid| cid.0)),
    }))?)
}
