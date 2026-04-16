// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use tokio::sync::Mutex;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct ToolCallTrace {
    pub tool_name: String,
    pub timestamp: DateTime<Utc>,
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub struct SessionContext {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub last_intent: Option<Value>,
    pub last_search_result_summary: Option<Value>,
    pub last_selected_cid: Option<String>,
    pub search_completed_for_current_intent: bool,
    pub call_trace: Vec<ToolCallTrace>,
}

impl SessionContext {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            created_at: Utc::now(),
            last_intent: None,
            last_search_result_summary: None,
            last_selected_cid: None,
            search_completed_for_current_intent: false,
            call_trace: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct SessionManager {
    sessions: Mutex<HashMap<String, SessionContext>>,
}

impl SessionManager {
    pub async fn touch(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().await;
        sessions
            .entry(session_id.to_string())
            .or_insert_with(|| SessionContext::new(session_id));
    }

    pub async fn duplicate_dataset_search_summary(&self, session_id: &str) -> Option<Value> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .entry(session_id.to_string())
            .or_insert_with(|| SessionContext::new(session_id));
        if !session.search_completed_for_current_intent {
            return None;
        }

        Some(json!({
            "message": "dataset_search has already completed for the current intent; summarize the existing workspace/results, or call intent_parse again to start a new discovery task",
            "last_search_result_summary": session
                .last_search_result_summary
                .clone()
                .unwrap_or(Value::Null),
        }))
    }

    pub async fn record_tool_call(
        &self,
        session_id: &str,
        tool_name: &str,
        args: &Value,
        raw_output: &str,
        is_error: bool,
    ) {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .entry(session_id.to_string())
            .or_insert_with(|| SessionContext::new(session_id));

        session.call_trace.push(ToolCallTrace {
            tool_name: tool_name.to_string(),
            timestamp: Utc::now(),
            is_error,
        });
        if session.call_trace.len() > 32 {
            let overflow = session.call_trace.len() - 32;
            session.call_trace.drain(0..overflow);
        }

        if let Some(cid) = args
            .get("cid")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            session.last_selected_cid = Some(cid.to_string());
        }

        if is_error {
            return;
        }

        let Some(parsed) = serde_json::from_str::<Value>(raw_output).ok() else {
            return;
        };

        match tool_name {
            "intent_parse" => {
                session.last_intent = parsed
                    .get("intent")
                    .cloned()
                    .or_else(|| Some(parsed.clone()));
                session.last_search_result_summary = None;
                session.search_completed_for_current_intent = false;
            }
            "dataset_search" => {
                session.last_search_result_summary = Some(json!({
                    "intent": parsed.get("intent").cloned().unwrap_or(Value::Null),
                    "results": parsed.get("results").cloned().unwrap_or_else(|| json!([])),
                    "errors": parsed.get("errors").cloned().unwrap_or_else(|| json!([])),
                    "workspace_meta": parsed.get("workspace_meta").cloned().unwrap_or(Value::Null),
                }));
                session.search_completed_for_current_intent = true;
                if let Some(cid) = parsed
                    .get("results")
                    .and_then(|value| value.as_array())
                    .and_then(|results| results.first())
                    .and_then(|result| result.get("cid"))
                    .and_then(|value| value.as_str())
                {
                    session.last_selected_cid = Some(cid.to_string());
                }
            }
            "pan_search" => {
                session.last_search_result_summary = Some(json!({
                    "results": parsed.get("results").cloned().unwrap_or_else(|| json!([])),
                }));
            }
            _ => {}
        }
    }

    #[allow(dead_code)]
    pub async fn get(&self, session_id: &str) -> Option<SessionContext> {
        let sessions = self.sessions.lock().await;
        sessions.get(session_id).cloned()
    }
}
