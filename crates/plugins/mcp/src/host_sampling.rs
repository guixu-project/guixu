// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use crate::protocol::JSONRPC_VERSION;

type PendingSamplingResult = std::result::Result<Value, String>;

#[derive(Default)]
pub struct HostSamplingRuntime {
    supports_sampling: AtomicBool,
    next_request_id: AtomicU64,
    outbound_tx: Mutex<Option<mpsc::UnboundedSender<Value>>>,
    pending: Mutex<HashMap<String, oneshot::Sender<PendingSamplingResult>>>,
}

impl HostSamplingRuntime {
    pub fn new() -> Self {
        Self {
            supports_sampling: AtomicBool::new(false),
            next_request_id: AtomicU64::new(1),
            outbound_tx: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_supports_sampling(&self, supports_sampling: bool) {
        self.supports_sampling
            .store(supports_sampling, Ordering::Relaxed);
    }

    pub fn supports_sampling(&self) -> bool {
        self.supports_sampling.load(Ordering::Relaxed)
    }

    pub fn attach_sender(&self, sender: mpsc::UnboundedSender<Value>) {
        let mut outbound_tx = self
            .outbound_tx
            .lock()
            .expect("sampling sender mutex poisoned");
        *outbound_tx = Some(sender);
    }

    pub fn detach_sender(&self) {
        let mut outbound_tx = self
            .outbound_tx
            .lock()
            .expect("sampling sender mutex poisoned");
        *outbound_tx = None;
    }

    pub fn handle_response(&self, payload: &Value) -> bool {
        if payload.get("method").is_some() {
            return false;
        }

        let Some(id) = payload.get("id") else {
            return false;
        };
        let key = id.to_string();
        let tx = {
            let mut pending = self
                .pending
                .lock()
                .expect("sampling pending mutex poisoned");
            pending.remove(&key)
        };

        let Some(tx) = tx else {
            return false;
        };

        let result = if let Some(error) = payload.get("error") {
            let message = error
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("sampling request failed")
                .to_string();
            Err(message)
        } else {
            Ok(payload.clone())
        };
        let _ = tx.send(result);
        true
    }

    pub fn shutdown(&self, reason: &str) {
        self.detach_sender();

        let pending = {
            let mut pending = self
                .pending
                .lock()
                .expect("sampling pending mutex poisoned");
            std::mem::take(&mut *pending)
        };

        for (_, tx) in pending {
            let _ = tx.send(Err(reason.to_string()));
        }
    }

    pub async fn create_message(&self, params: CreateMessageParams) -> Result<CreateMessageResult> {
        if !self.supports_sampling() {
            bail!("MCP client does not advertise sampling support");
        }

        let sender = {
            let outbound_tx = self
                .outbound_tx
                .lock()
                .expect("sampling sender mutex poisoned");
            outbound_tx
                .clone()
                .ok_or_else(|| anyhow!("sampling transport is not attached"))?
        };

        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let id_value = Value::from(request_id);
        let key = id_value.to_string();
        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self
                .pending
                .lock()
                .expect("sampling pending mutex poisoned");
            pending.insert(key.clone(), tx);
        }

        let payload = json!({
            "jsonrpc": JSONRPC_VERSION,
            "id": id_value,
            "method": "sampling/createMessage",
            "params": params,
        });

        if sender.send(payload).is_err() {
            let mut pending = self
                .pending
                .lock()
                .expect("sampling pending mutex poisoned");
            pending.remove(&key);
            bail!("failed to send sampling request to MCP client");
        }

        let response = rx
            .await
            .context("sampling response channel closed")?
            .map_err(|message| anyhow!(message))?;

        let result = response
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow!("sampling response did not include a result"))?;
        serde_json::from_value(result).context("parse sampling response payload")
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateMessageParams {
    pub messages: Vec<SamplingMessage>,
    #[serde(
        rename = "modelPreferences",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub model_preferences: Option<ModelPreferences>,
    #[serde(
        rename = "systemPrompt",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub system_prompt: Option<String>,
    #[serde(rename = "maxTokens", skip_serializing_if = "Option::is_none", default)]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SamplingMessage {
    pub role: String,
    pub content: SamplingContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SamplingContent {
    Text {
        text: String,
    },
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelPreferences {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<ModelHint>,
    #[serde(
        rename = "costPriority",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub cost_priority: Option<f32>,
    #[serde(
        rename = "speedPriority",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub speed_priority: Option<f32>,
    #[serde(
        rename = "intelligencePriority",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub intelligence_priority: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelHint {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMessageResult {
    pub role: String,
    pub content: SamplingContent,
    pub model: String,
    #[serde(rename = "stopReason", default)]
    pub stop_reason: Option<String>,
}
