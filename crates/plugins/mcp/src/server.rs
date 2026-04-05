// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::RwLock;

use crate::catalog::build_registry;
use crate::registry::ToolRegistry;
use crate::sampling::SamplingHandle;
use crate::session::SessionManager;

pub use crate::http::run_http;
pub use crate::state::AppState;
pub use crate::stdio::run_stdio;

pub struct McpServer {
    state: AppState,
    registry: ToolRegistry,
    sessions: SessionManager,
    sampling_handle: RwLock<Option<SamplingHandle>>,
    host_supports_sampling: RwLock<bool>,
}

impl McpServer {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            registry: build_registry(),
            sessions: SessionManager::default(),
            sampling_handle: RwLock::new(None),
            host_supports_sampling: RwLock::new(false),
        }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    pub fn sessions(&self) -> &SessionManager {
        &self.sessions
    }

    /// Store the sampling handle created by the stdio/http transport.
    pub fn set_sampling_handle(&self, handle: SamplingHandle) {
        // Store in both server and app state so tool handlers can access it.
        *self.state.sampling_handle.write().unwrap() = Some(handle.clone());
        *self.sampling_handle.write().unwrap() = Some(handle);
    }

    /// Record whether the host advertised sampling capability during initialize.
    pub fn set_host_supports_sampling(&self, supports: bool) {
        *self.host_supports_sampling.write().unwrap() = supports;
    }

    /// Returns true if the host supports MCP sampling.
    pub fn host_supports_sampling(&self) -> bool {
        *self.host_supports_sampling.read().unwrap()
    }

    /// Get a clone of the sampling handle, if available.
    pub fn sampling_handle(&self) -> Option<SamplingHandle> {
        self.sampling_handle.read().unwrap().clone()
    }
}
