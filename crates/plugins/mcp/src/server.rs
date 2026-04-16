// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use crate::catalog::build_registry;
use crate::registry::ToolRegistry;
use crate::session::SessionManager;

pub use crate::http::run_http;
pub use crate::state::AppState;
pub use crate::stdio::run_stdio;

use data_storage::trace_pool::TracePool;

pub struct McpServer {
    state: AppState,
    registry: ToolRegistry,
    sessions: SessionManager,
    trace_pool: Option<TracePool>,
}

impl McpServer {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            registry: build_registry(),
            sessions: SessionManager::default(),
            trace_pool: None,
        }
    }

    /// Initialize the DuckDB connection pool for trace API endpoints.
    pub fn init_trace_pool(&mut self, pool_size: usize) -> anyhow::Result<()> {
        let db_path = data_core::config::NodeConfig::load_or_default()
            .trace
            .db_path;
        let pool = TracePool::open(std::path::Path::new(&db_path), pool_size)?;
        self.trace_pool = Some(pool);
        Ok(())
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

    pub fn trace_pool(&self) -> Option<&TracePool> {
        self.trace_pool.as_ref()
    }
}
