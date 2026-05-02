// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Common types for external agent control.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a task.
pub type TaskId = String;

/// Status of a task execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    /// Task is pending execution.
    Pending,
    /// Task is currently running.
    Running,
    /// Task completed successfully.
    Completed,
    /// Task failed.
    Failed,
    /// Task was cancelled.
    Cancelled,
    /// Task timed out.
    TimedOut,
}

/// A task to be executed by an external agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    /// Unique task identifier.
    pub id: TaskId,
    /// Task description or prompt.
    pub description: String,
    /// Additional parameters for the task.
    pub parameters: HashMap<String, serde_json::Value>,
    /// Maximum execution time in seconds.
    pub timeout_secs: Option<u64>,
    /// Priority (higher values = higher priority).
    pub priority: i32,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl AgentTask {
    /// Create a new task with a description.
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            parameters: HashMap::new(),
            timeout_secs: None,
            priority: 0,
            created_at: Utc::now(),
        }
    }

    /// Set a parameter.
    pub fn with_parameter(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.parameters.insert(key.into(), value);
        self
    }

    /// Set timeout.
    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_secs = Some(seconds);
        self
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// Response from an external agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    /// Task identifier.
    pub task_id: TaskId,
    /// Status of the task.
    pub status: TaskStatus,
    /// Response content (if any).
    pub content: Option<String>,
    /// Structured data (if any).
    pub data: Option<serde_json::Value>,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Execution duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// Timestamp when response was received.
    pub received_at: DateTime<Utc>,
}

impl AgentResponse {
    /// Create a successful response.
    pub fn success(task_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            status: TaskStatus::Completed,
            content: Some(content.into()),
            data: None,
            error: None,
            duration_ms: None,
            received_at: Utc::now(),
        }
    }

    /// Create a failed response.
    pub fn failed(task_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            status: TaskStatus::Failed,
            content: None,
            data: None,
            error: Some(error.into()),
            duration_ms: None,
            received_at: Utc::now(),
        }
    }

    /// Check if the task was successful.
    pub fn is_success(&self) -> bool {
        self.status == TaskStatus::Completed
    }
}

/// Health status of an external agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHealth {
    /// Agent identifier.
    pub agent_id: String,
    /// Whether the agent is reachable.
    pub is_reachable: bool,
    /// Agent version (if available).
    pub version: Option<String>,
    /// Uptime in seconds (if available).
    pub uptime_secs: Option<u64>,
    /// Additional metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}
