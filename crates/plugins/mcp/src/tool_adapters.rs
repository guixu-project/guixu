use std::sync::Arc;

use crate::protocol::ToolDefinition;
use crate::registry::{RegisteredTool, ToolExecutor};

pub fn legacy_json_tool(definition: ToolDefinition, executor: Arc<ToolExecutor>) -> RegisteredTool {
    RegisteredTool::new(definition, executor)
}
