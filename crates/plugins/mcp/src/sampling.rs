// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! MCP sampling client — sends `sampling/createMessage` requests to the host
//! so that all LLM inference is performed by the host AI client rather than
//! calling third-party LLM APIs directly.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

// ---------------------------------------------------------------------------
// Public error code returned when the host does not support sampling.
// ---------------------------------------------------------------------------

pub const SAMPLING_NOT_SUPPORTED_CODE: &str = "SAMPLING_NOT_SUPPORTED";

pub const SAMPLING_NOT_SUPPORTED_MSG: &str =
    "Guixu requires the host AI client to support MCP sampling capability. \
     Your current client does not advertise sampling support. \
     Please use a client that supports MCP sampling, or contact the client \
     vendor to request this feature.";

// ---------------------------------------------------------------------------
// Wire types for `sampling/createMessage`
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct SamplingRequest {
    pub messages: Vec<SamplingMessage>,
    #[serde(rename = "systemPrompt", skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(rename = "maxTokens")]
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingMessage {
    pub role: String,
    pub content: SamplingContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingContent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SamplingResponse {
    pub model: Option<String>,
    pub content: SamplingContent,
}

// ---------------------------------------------------------------------------
// Transport: a channel pair that the stdio / http loop feeds.
// ---------------------------------------------------------------------------

/// A pending sampling request waiting for the host's response.
pub struct PendingSampling {
    pub id: serde_json::Value,
    pub body: serde_json::Value,
    pub reply: oneshot::Sender<Result<serde_json::Value>>,
}

/// Handle held by tool executors to issue sampling requests.
#[derive(Clone)]
pub struct SamplingHandle {
    tx: mpsc::Sender<PendingSampling>,
    next_id: Arc<AtomicU64>,
}

impl SamplingHandle {
    pub fn new(tx: mpsc::Sender<PendingSampling>) -> Self {
        Self {
            tx,
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Send a `sampling/createMessage` request to the host and wait for the
    /// response.  Returns the parsed `SamplingResponse`.
    pub async fn create_message(&self, req: SamplingRequest) -> Result<SamplingResponse> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id_value = serde_json::Value::Number(id.into());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id_value,
            "method": "sampling/createMessage",
            "params": serde_json::to_value(&req)?,
        });

        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(PendingSampling {
                id: id_value,
                body,
                reply: reply_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("sampling transport closed"))?;

        let result = reply_rx
            .await
            .map_err(|_| anyhow::anyhow!("sampling response channel dropped"))??;

        serde_json::from_value::<SamplingResponse>(result)
            .context("parse sampling/createMessage response")
    }

    /// Convenience: send a system+user prompt pair and return the text content.
    pub async fn chat_text(
        &self,
        system_prompt: &str,
        user_prompt: String,
        max_tokens: u32,
    ) -> Result<String> {
        let req = SamplingRequest {
            messages: vec![SamplingMessage {
                role: "user".into(),
                content: SamplingContent {
                    kind: "text".into(),
                    text: Some(user_prompt),
                    data: None,
                    mime_type: None,
                },
            }],
            system_prompt: Some(system_prompt.into()),
            max_tokens,
        };
        let resp = self.create_message(req).await?;
        resp.content
            .text
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("host sampling returned empty text"))
    }

    /// Convenience: send a system+user prompt pair expecting JSON, parse into T.
    pub async fn chat_json<T: serde::de::DeserializeOwned>(
        &self,
        system_prompt: &str,
        user_prompt: String,
        max_tokens: u32,
    ) -> Result<T> {
        let text = self
            .chat_text(system_prompt, user_prompt, max_tokens)
            .await?;
        serde_json::from_str(&text)
            .with_context(|| format!("parse host sampling JSON response: {text}"))
    }
}

// ---------------------------------------------------------------------------
// Helpers for content construction
// ---------------------------------------------------------------------------

impl SamplingContent {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            kind: "text".into(),
            text: Some(text.into()),
            data: None,
            mime_type: None,
        }
    }

    pub fn image(base64_data: String, mime_type: String) -> Self {
        Self {
            kind: "image".into(),
            text: None,
            data: Some(base64_data),
            mime_type: Some(mime_type),
        }
    }
}
