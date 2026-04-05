// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use crate::catalog::build_registry;
use crate::registry::ToolRegistry;
use crate::session::SessionManager;

pub use crate::http::run_http;
pub use crate::state::AppState;
pub use crate::stdio::run_stdio;

pub struct McpServer {
    state: AppState,
    registry: ToolRegistry,
    sessions: SessionManager,
}

impl McpServer {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            registry: build_registry(),
            sessions: SessionManager::default(),
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
}
