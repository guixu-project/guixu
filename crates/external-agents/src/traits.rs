// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Core traits for external agent control.

use async_trait::async_trait;

use crate::error::Result;
use crate::types::{AgentHealth, AgentResponse, AgentTask};

/// Trait for controlling external AI agents.
///
/// This trait defines the common interface for interacting with different
/// AI agents like hermes-agent, openclaw, etc.
#[async_trait]
pub trait ExternalAgent: Send + Sync {
    /// Get the agent identifier.
    fn agent_id(&self) -> &str;

    /// Get the agent type (e.g., "hermes", "openclaw", "custom").
    fn agent_type(&self) -> &str;

    /// Check if the agent is healthy and reachable.
    async fn health_check(&self) -> Result<AgentHealth>;

    /// Execute a task on the agent.
    ///
    /// This is the main method for sending tasks to the agent.
    /// The implementation should handle timeout, retries, and error handling.
    async fn execute_task(&self, task: AgentTask) -> Result<AgentResponse>;

    /// Execute a simple text prompt.
    ///
    /// Convenience method that creates a task from a text prompt.
    async fn prompt(&self, prompt: &str) -> Result<AgentResponse> {
        let task = AgentTask::new(prompt);
        self.execute_task(task).await
    }

    /// Execute a task with timeout.
    async fn execute_task_with_timeout(
        &self,
        task: AgentTask,
        timeout_secs: u64,
    ) -> Result<AgentResponse> {
        let task = task.with_timeout(timeout_secs);
        self.execute_task(task).await
    }

    /// Cancel a running task (if supported).
    async fn cancel_task(&self, _task_id: &str) -> Result<()> {
        Err(crate::error::ExternalAgentError::UnsupportedOperation(
            "Task cancellation not supported".to_string(),
        ))
    }

    /// Get task status (if supported).
    async fn get_task_status(&self, _task_id: &str) -> Result<AgentResponse> {
        Err(crate::error::ExternalAgentError::UnsupportedOperation(
            "Task status query not supported".to_string(),
        ))
    }

    /// Get supported capabilities.
    fn capabilities(&self) -> Vec<AgentCapability> {
        vec![]
    }
}

/// Capabilities that an agent might support.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentCapability {
    /// Can execute text prompts.
    TextPrompt,
    /// Can execute shell commands.
    ShellCommand,
    /// Can browse the web.
    WebBrowsing,
    /// Can read/write files.
    FileSystem,
    /// Can run code.
    CodeExecution,
    /// Supports task cancellation.
    TaskCancellation,
    /// Supports task status queries.
    TaskStatus,
    /// Supports streaming responses.
    Streaming,
    /// Custom capability.
    Custom(String),
}

/// Factory for creating external agents.
pub struct AgentFactory;

impl AgentFactory {
    /// Create an external agent from configuration.
    ///
    /// The factory creates the appropriate adapter based on the connection type:
    /// - `ConnectionConfig::Http` creates an HTTP adapter
    /// - `ConnectionConfig::Cli` creates a CLI adapter
    pub fn create(config: &crate::config::ExternalAgentConfig) -> Result<Box<dyn ExternalAgent>> {
        match &config.connection {
            crate::config::ConnectionConfig::Http(_) => {
                let adapter = crate::adapters::http::HttpAdapter::new(config)?;
                Ok(Box::new(adapter))
            }
            crate::config::ConnectionConfig::Cli(_) => {
                let adapter = crate::adapters::cli::CliAdapter::new(config)?;
                Ok(Box::new(adapter))
            }
            crate::config::ConnectionConfig::Custom(_) => {
                Err(crate::error::ExternalAgentError::Config(
                    "Custom connection type not yet supported".to_string(),
                ))
            }
        }
    }
}
