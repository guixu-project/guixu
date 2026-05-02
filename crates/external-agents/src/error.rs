// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Error types for external agent control.

use thiserror::Error;

/// Result type for external agent operations.
pub type Result<T> = std::result::Result<T, ExternalAgentError>;

/// Errors that can occur when controlling external agents.
#[derive(Error, Debug)]
pub enum ExternalAgentError {
    /// HTTP request failed.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// Invalid URL.
    #[error("Invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// IO error (for CLI execution).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Agent execution timeout.
    #[error("Agent execution timed out after {0} seconds")]
    Timeout(u64),

    /// Agent returned an error response.
    #[error("Agent error: {status_code} - {message}")]
    AgentResponse { status_code: u16, message: String },

    /// Task failed.
    #[error("Task failed: {0}")]
    TaskFailed(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Authentication error.
    #[error("Authentication error: {0}")]
    Auth(String),

    /// Agent not found.
    #[error("Agent not found: {0}")]
    NotFound(String),

    /// Unsupported operation.
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    /// Generic error.
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
