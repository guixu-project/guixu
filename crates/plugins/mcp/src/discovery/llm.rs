// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;

use crate::host_sampling::{
    CreateMessageParams, HostSamplingRuntime, ModelHint, ModelPreferences, SamplingContent,
    SamplingMessage,
};

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete_text(&self, system_prompt: &str, user_prompt: &str) -> Result<String>;
}

pub struct OpenAiCompatibleProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
}

impl OpenAiCompatibleProvider {
    pub fn from_env(prefix: &str) -> Result<Self> {
        Self::from_env_with_fallback(prefix, prefix)
    }

    pub fn from_env_with_fallback(primary: &str, fallback: &str) -> Result<Self> {
        let base_url = read_setting(primary, fallback, "BASE_URL")
            .context("missing LLM base URL configuration")?;
        let model =
            read_setting(primary, fallback, "MODEL").context("missing LLM model configuration")?;
        let api_key = read_setting(primary, fallback, "API_KEY");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(45))
            .user_agent("guixu-discovery/0.1")
            .build()
            .context("build LLM HTTP client")?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
            api_key,
        })
    }
}

pub struct HostSamplingProvider {
    runtime: std::sync::Arc<HostSamplingRuntime>,
}

impl HostSamplingProvider {
    pub fn new(runtime: std::sync::Arc<HostSamplingRuntime>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    async fn complete_text(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut request = self
            .client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .json(&json!({
                "model": self.model,
                "temperature": 0.1,
                "messages": [
                    { "role": "system", "content": system_prompt },
                    { "role": "user", "content": user_prompt }
                ]
            }));
        if let Some(api_key) = self.api_key.as_deref().filter(|key| !key.is_empty()) {
            request = request.header(AUTHORIZATION, format!("Bearer {api_key}"));
        }

        let response = request.send().await.context("send LLM request")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("LLM request failed ({status}): {body}");
        }

        let payload: serde_json::Value =
            response.json().await.context("parse LLM response JSON")?;
        extract_content(&payload).context("extract LLM completion text")
    }
}

#[async_trait]
impl LlmProvider for HostSamplingProvider {
    async fn complete_text(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let response = self
            .runtime
            .create_message(CreateMessageParams {
                messages: vec![SamplingMessage {
                    role: "user".to_string(),
                    content: SamplingContent::Text {
                        text: user_prompt.to_string(),
                    },
                }],
                model_preferences: Some(ModelPreferences {
                    hints: vec![ModelHint {
                        name: "codex".to_string(),
                    }],
                    cost_priority: Some(0.2),
                    speed_priority: Some(0.2),
                    intelligence_priority: Some(1.0),
                }),
                system_prompt: Some(system_prompt.to_string()),
                max_tokens: Some(900),
            })
            .await?;

        match response.content {
            SamplingContent::Text { text } => Ok(text),
            SamplingContent::Image { .. } => {
                bail!("sampling response returned image content instead of text")
            }
        }
    }
}

fn read_setting(primary: &str, fallback: &str, key: &str) -> Option<String> {
    let primary_key = format!("{primary}_{key}");
    std::env::var(&primary_key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let fallback_key = format!("{fallback}_{key}");
            std::env::var(&fallback_key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

fn extract_content(payload: &serde_json::Value) -> Option<String> {
    let content = payload
        .get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))?;

    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }

    content.as_array().map(|parts| {
        parts
            .iter()
            .filter_map(|part| part.get("text").and_then(|value| value.as_str()))
            .collect::<String>()
    })
}
