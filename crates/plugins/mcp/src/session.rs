use std::collections::HashMap;
use std::sync::Mutex;

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
            call_trace: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct SessionManager {
    sessions: Mutex<HashMap<String, SessionContext>>,
}

impl SessionManager {
    pub fn touch(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().expect("session mutex poisoned");
        sessions
            .entry(session_id.to_string())
            .or_insert_with(|| SessionContext::new(session_id));
    }

    pub fn record_tool_call(
        &self,
        session_id: &str,
        tool_name: &str,
        args: &Value,
        raw_output: &str,
        is_error: bool,
    ) {
        let mut sessions = self.sessions.lock().expect("session mutex poisoned");
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
            }
            "dataset_search" => {
                session.last_search_result_summary = Some(json!({
                    "intent": parsed.get("intent").cloned().unwrap_or(Value::Null),
                    "results": parsed.get("results").cloned().unwrap_or_else(|| json!([])),
                    "errors": parsed.get("errors").cloned().unwrap_or_else(|| json!([])),
                }));
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
            _ => {}
        }
    }

    #[allow(dead_code)]
    pub fn get(&self, session_id: &str) -> Option<SessionContext> {
        let sessions = self.sessions.lock().expect("session mutex poisoned");
        sessions.get(session_id).cloned()
    }
}
