// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Configuration for external agents.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::Result;

/// Configuration for an external agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalAgentConfig {
    /// Unique identifier for this agent configuration.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Agent type: "openclaw", "hermes", or custom.
    pub agent_type: String,
    /// Connection configuration.
    pub connection: ConnectionConfig,
    /// Default timeout in seconds.
    pub default_timeout_secs: u64,
    /// Maximum retries for failed requests.
    pub max_retries: u32,
    /// Authentication configuration (if needed).
    pub auth: Option<AuthConfig>,
    /// Additional agent-specific configuration.
    pub extra: HashMap<String, serde_json::Value>,
}

/// Connection configuration for different agent types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionConfig {
    /// HTTP connection (for openclaw and similar).
    Http(HttpConnection),
    /// CLI execution (for hermes-agent and similar).
    Cli(CliConnection),
    /// Custom connection.
    Custom(HashMap<String, serde_json::Value>),
}

/// HTTP connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConnection {
    /// Base URL of the agent's API.
    pub base_url: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Whether to verify SSL certificates.
    pub verify_ssl: bool,
    /// Additional headers to send with requests.
    pub headers: HashMap<String, String>,
}

/// CLI connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConnection {
    /// Path to the agent executable.
    pub executable: PathBuf,
    /// Working directory for command execution.
    pub working_dir: Option<PathBuf>,
    /// Environment variables to set.
    pub env_vars: HashMap<String, String>,
    /// Shell to use for command execution.
    pub shell: Option<String>,
    /// Whether to capture stderr.
    pub capture_stderr: bool,
}

/// Authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthConfig {
    /// Bearer token authentication.
    BearerToken {
        token: String,
        header_name: Option<String>,
    },
    /// Basic authentication.
    Basic { username: String, password: String },
    /// API key authentication.
    ApiKey { key: String, header_name: String },
    /// Custom authentication.
    Custom(HashMap<String, String>),
}

impl ExternalAgentConfig {
    /// Create a new configuration with defaults.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        agent_type: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            agent_type: agent_type.into(),
            connection: ConnectionConfig::Http(HttpConnection {
                base_url: "http://localhost:18789".to_string(),
                timeout_secs: 30,
                verify_ssl: true,
                headers: HashMap::new(),
            }),
            default_timeout_secs: 60,
            max_retries: 3,
            auth: None,
            extra: HashMap::new(),
        }
    }

    /// Create an OpenClaw configuration.
    pub fn openclaw(id: impl Into<String>, base_url: impl Into<String>) -> Self {
        let mut config = Self::new(id, "OpenClaw Agent", "openclaw");
        config.connection = ConnectionConfig::Http(HttpConnection {
            base_url: base_url.into(),
            timeout_secs: 30,
            verify_ssl: true,
            headers: HashMap::new(),
        });
        config
    }

    /// Create a Hermes configuration.
    pub fn hermes(id: impl Into<String>, executable: impl Into<PathBuf>) -> Self {
        let mut config = Self::new(id, "Hermes Agent", "hermes");
        config.connection = ConnectionConfig::Cli(CliConnection {
            executable: executable.into(),
            working_dir: None,
            env_vars: HashMap::new(),
            shell: None,
            capture_stderr: true,
        });
        config
    }

    /// Set authentication with bearer token.
    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(AuthConfig::BearerToken {
            token: token.into(),
            header_name: None,
        });
        self
    }

    /// Set authentication with API key.
    pub fn with_api_key(mut self, key: impl Into<String>, header: impl Into<String>) -> Self {
        self.auth = Some(AuthConfig::ApiKey {
            key: key.into(),
            header_name: header.into(),
        });
        self
    }

    /// Set default timeout.
    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.default_timeout_secs = seconds;
        self
    }

    /// Load configuration from a file.
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::error::ExternalAgentError::Config(e.to_string()))?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to a file.
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)
            .map_err(|e| crate::error::ExternalAgentError::Config(e.to_string()))?;
        Ok(())
    }
}
