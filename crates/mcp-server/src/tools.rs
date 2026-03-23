use serde_json::json;

use crate::protocol::ToolDefinition;

/// Returns all MCP tool definitions exposed by this server.
pub fn all_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dataset_search".into(),
            description: "Search datasets with natural language and structured filters".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural language search query" },
                    "filters": {
                        "type": "object",
                        "properties": {
                            "topic": { "type": "string" },
                            "min_rows": { "type": "integer" },
                            "max_price": { "type": "number" },
                            "license": { "type": "string" },
                            "min_quality": { "type": "number" }
                        }
                    },
                    "limit": { "type": "integer", "default": 10 }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "dataset_preview".into(),
            description: "Preview sample rows from a dataset (micropayment may apply)".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "cid": { "type": "string" },
                    "rows": { "type": "integer", "default": 10 }
                },
                "required": ["cid"]
            }),
        },
        ToolDefinition {
            name: "dataset_purchase".into(),
            description: "Purchase and download a dataset (escrowed transaction)".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "cid": { "type": "string" },
                    "max_price": { "type": "number" }
                },
                "required": ["cid"]
            }),
        },
        ToolDefinition {
            name: "dataset_verify".into(),
            description: "Verify dataset integrity and provenance".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "cid": { "type": "string" },
                    "check_chain": { "type": "boolean", "default": false }
                },
                "required": ["cid"]
            }),
        },
        ToolDefinition {
            name: "dataset_evaluate".into(),
            description: "Evaluate dataset quality, value, and task fitness".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "cid": { "type": "string" },
                    "use_case": { "type": "string" },
                    "agent_context": {
                        "type": "object",
                        "properties": {
                            "task_description": { "type": "string" },
                            "existing_data_cids": { "type": "array", "items": { "type": "string" } },
                            "budget": { "type": "number" }
                        }
                    }
                },
                "required": ["cid"]
            }),
        },
        ToolDefinition {
            name: "memory_evaluate".into(),
            description: "Evaluate Agent Memory/Skill fitness for current task".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "memory_cid": { "type": "string" },
                    "task_description": { "type": "string" },
                    "agent_capabilities": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["memory_cid", "task_description"]
            }),
        },
        ToolDefinition {
            name: "dataset_publish".into(),
            description: "Publish a local dataset to the P2P network".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string" },
                    "metadata": {
                        "type": "object",
                        "properties": {
                            "title": { "type": "string" },
                            "description": { "type": "string" },
                            "license": { "type": "string" },
                            "price": { "type": "number" },
                            "tags": { "type": "array", "items": { "type": "string" } }
                        }
                    }
                },
                "required": ["file_path", "metadata"]
            }),
        },
    ]
}
