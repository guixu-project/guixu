// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! External AI Agent Control Module
//!
//! This module provides abstractions for controlling external AI agents
//! like hermes-agent and openclaw through HTTP APIs and CLI interfaces.

pub mod config;
pub mod error;
pub mod mcp_client;
pub mod registry;
pub mod traits;
pub mod types;

pub mod adapters;

// Re-export main types
pub use config::ExternalAgentConfig;
pub use error::{ExternalAgentError, Result};
pub use mcp_client::GuixuMcpClient;
pub use registry::AgentRegistry;
pub use traits::ExternalAgent;
pub use types::{AgentResponse, AgentTask, TaskStatus};
