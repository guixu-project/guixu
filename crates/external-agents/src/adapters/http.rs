// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Generic HTTP adapter for external AI agents.

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;
use tracing::{debug, info};

use crate::config::{AuthConfig, ConnectionConfig, ExternalAgentConfig};
use crate::error::{ExternalAgentError, Result};
use crate::traits::{AgentCapability, ExternalAgent};
use crate::types::{AgentHealth, AgentResponse, AgentTask};

/// Generic HTTP adapter for controlling external AI agents via HTTP APIs.
///
/// This adapter can work with any AI agent that exposes an HTTP API,
/// including but not limited to OpenAI-compatible endpoints.
pub struct HttpAdapter {
    config: ExternalAgentConfig,
    client: Client,
    base_url: String,
}

impl HttpAdapter {
    /// Create a new HTTP adapter from configuration.
    pub fn new(config: &ExternalAgentConfig) -> Result<Self> {
        let http_config = match &config.connection {
            ConnectionConfig::Http(http) => http.clone(),
            _ => {
                return Err(ExternalAgentError::Config(
                    "HTTP adapter requires HTTP connection configuration".to_string(),
                ))
            }
        };

        let client = Client::builder()
            .timeout(Duration::from_secs(http_config.timeout_secs))
            .danger_accept_invalid_certs(!http_config.verify_ssl)
            .build()?;

        Ok(Self {
            config: config.clone(),
            client,
            base_url: http_config.base_url.trim_end_matches('/').to_string(),
        })
    }

    /// Get authentication headers from configuration.
    fn auth_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();

        if let Some(ref auth) = self.config.auth {
            match auth {
                AuthConfig::BearerToken { token, header_name } => {
                    let header_name = header_name
                        .clone()
                        .unwrap_or_else(|| "Authorization".to_string());
                    headers.insert(
                        reqwest::header::HeaderName::from_bytes(header_name.as_bytes()).unwrap(),
                        format!("Bearer {}", token).parse().unwrap(),
                    );
                }
                AuthConfig::ApiKey { key, header_name } => {
                    headers.insert(
                        reqwest::header::HeaderName::from_bytes(header_name.as_bytes()).unwrap(),
                        key.parse().unwrap(),
                    );
                }
                _ => {}
            }
        }

        // Add custom headers from configuration
        if let ConnectionConfig::Http(ref http_config) = self.config.connection {
            for (key, value) in &http_config.headers {
                if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(key.as_bytes()) {
                    headers.insert(header_name, value.parse().unwrap());
                }
            }
        }

        headers
    }

    /// Make a generic HTTP POST request to the agent.
    async fn make_request(
        &self,
        endpoint: &str,
        body: &Value,
        timeout_secs: u64,
    ) -> Result<AgentResponse> {
        let url = format!("{}{}", self.base_url, endpoint);

        debug!("Sending HTTP request to {}: {}", url, body);

        let response = self
            .client
            .post(&url)
            .headers(self.auth_headers())
            .json(body)
            .timeout(Duration::from_secs(timeout_secs))
            .send()
            .await?;

        let status = response.status();
        let response_body: Value = response.json().await?;

        if !status.is_success() {
            let error_message = response_body
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .or_else(|| response_body.get("error").and_then(|e| e.as_str()))
                .unwrap_or("Unknown error");

            return Ok(AgentResponse::failed(
                "", // Task ID will be set by caller
                format!("{}: {}", status.as_u16(), error_message),
            ));
        }

        // Try to extract content from common response formats
        let content = self.extract_response_content(&response_body);

        Ok(AgentResponse::success("", content))
    }

    /// Extract response content from common API response formats.
    fn extract_response_content(&self, body: &Value) -> String {
        // Try OpenAI-compatible format first
        if let Some(content) = body
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
        {
            return content.to_string();
        }

        // Try simple content field
        if let Some(content) = body.get("content").and_then(|c| c.as_str()) {
            return content.to_string();
        }

        // Try message field
        if let Some(content) = body.get("message").and_then(|m| m.as_str()) {
            return content.to_string();
        }

        // Fall back to stringified JSON
        serde_json::to_string_pretty(body).unwrap_or_else(|_| body.to_string())
    }
}

#[async_trait]
impl ExternalAgent for HttpAdapter {
    fn agent_id(&self) -> &str {
        &self.config.id
    }

    fn agent_type(&self) -> &str {
        &self.config.agent_type
    }

    async fn health_check(&self) -> Result<AgentHealth> {
        // Try common health check endpoints
        let endpoints = ["/health", "/v1/models", "/status", "/"];
        let mut is_reachable = false;
        let mut metadata = std::collections::HashMap::new();

        for endpoint in &endpoints {
            let url = format!("{}{}", self.base_url, endpoint);
            if let Ok(response) = self
                .client
                .get(&url)
                .headers(self.auth_headers())
                .timeout(Duration::from_secs(5))
                .send()
                .await
            {
                if response.status().is_success() {
                    is_reachable = true;
                    if let Ok(body) = response.json::<Value>().await {
                        // Try to extract version info
                        if let Some(version) = body.get("version").and_then(|v| v.as_str()) {
                            metadata.insert("version".to_string(), json!(version));
                        }
                    }
                    break;
                }
            }
        }

        Ok(AgentHealth {
            agent_id: self.config.id.clone(),
            is_reachable,
            version: metadata
                .get("version")
                .and_then(|v| v.as_str())
                .map(String::from),
            uptime_secs: None,
            metadata,
        })
    }

    async fn execute_task(&self, task: AgentTask) -> Result<AgentResponse> {
        info!(
            "Executing task {} on HTTP agent {}",
            task.id, self.config.id
        );

        let timeout = task
            .timeout_secs
            .unwrap_or(self.config.default_timeout_secs);

        // Determine endpoint from task parameters
        let endpoint = task
            .parameters
            .get("endpoint")
            .and_then(|v| v.as_str())
            .unwrap_or("/v1/chat/completions");

        // Build request body from task
        let request_body = if task.parameters.contains_key("custom_body") {
            // Use custom body if provided
            task.parameters
                .get("custom_body")
                .cloned()
                .unwrap_or_default()
        } else {
            // Build standard chat completion request
            json!({
                "model": task.parameters.get("model").and_then(|v| v.as_str()).unwrap_or("default"),
                "messages": [
                    {
                        "role": "user",
                        "content": task.description
                    }
                ],
                "stream": false
            })
        };

        let mut response = self.make_request(endpoint, &request_body, timeout).await?;
        response.task_id = task.id;
        Ok(response)
    }

    fn capabilities(&self) -> Vec<AgentCapability> {
        vec![
            AgentCapability::TextPrompt,
            AgentCapability::ShellCommand,
            AgentCapability::WebBrowsing,
            AgentCapability::FileSystem,
            AgentCapability::CodeExecution,
            AgentCapability::TaskStatus,
        ]
    }
}
