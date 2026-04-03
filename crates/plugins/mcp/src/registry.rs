// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;

use crate::protocol::ToolDefinition;
use crate::state::AppState;

pub type ToolFuture<'a> = Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;
pub type ToolExecutor =
    dyn for<'a> Fn(serde_json::Value, &'a AppState) -> ToolFuture<'a> + Send + Sync;

pub struct RegisteredTool {
    definition: ToolDefinition,
    executor: Arc<ToolExecutor>,
}

impl RegisteredTool {
    pub fn new(definition: ToolDefinition, executor: Arc<ToolExecutor>) -> Self {
        Self {
            definition,
            executor,
        }
    }

    pub fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    pub async fn execute(&self, args: serde_json::Value, state: &AppState) -> Result<String> {
        (self.executor)(args, state).await
    }
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: Vec<RegisteredTool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: RegisteredTool) {
        self.tools.push(tool);
    }

    pub fn get(&self, name: &str) -> Option<&RegisteredTool> {
        self.tools
            .iter()
            .find(|tool| tool.definition().name == name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn list_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .iter()
            .map(|tool| tool.definition().clone())
            .collect()
    }
}
