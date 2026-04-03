// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::protocol::ToolDefinition;
use crate::registry::{RegisteredTool, ToolExecutor};

pub fn legacy_json_tool(definition: ToolDefinition, executor: Arc<ToolExecutor>) -> RegisteredTool {
    RegisteredTool::new(definition, executor)
}
