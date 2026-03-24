use serde_json::json;

use crate::protocol::ToolDefinition;

/// Returns all MCP tool definitions exposed by this server.
pub fn all_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dataset_search".into(),
            description: "Search datasets across Kaggle, HuggingFace, IPFS, BitTorrent, PostgreSQL, DuckDB and P2P network".into(),
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
                            "min_quality": { "type": "number" },
                            "source": { "type": "string", "enum": ["kaggle", "huggingface", "ipfs", "bittorrent", "postgresql", "duckdb", "p2p"] }
                        }
                    },
                    "limit": { "type": "integer", "default": 10 }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "dataset_evaluate".into(),
            description: "Compute Task-Conditioned Value (TCV) for a dataset. Returns a score from -100 (harmful) to +100 (highly valuable) based on schema fit, quality, and on-chain community feedback.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "cid": { "type": "string", "description": "Dataset content identifier" },
                    "task_description": { "type": "string", "description": "What the agent needs the data for" },
                    "task_type": { "type": "string", "description": "Task category (e.g. time_series_prediction, classification)" },
                    "required_columns": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Column names the task requires"
                    },
                    "budget": { "type": "number", "description": "Maximum budget in USD" }
                },
                "required": ["cid", "task_description"]
            }),
        },
        ToolDefinition {
            name: "dataset_feedback".into(),
            description: "Submit on-chain feedback after using a dataset. Recorded as an EAS attestation to help future agents evaluate this dataset.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "cid": { "type": "string", "description": "Dataset CID" },
                    "relevance_score": { "type": "number", "description": "-1.0 (harmful) to 1.0 (perfectly relevant)" },
                    "quality_rating": { "type": "integer", "description": "1-5 star rating" },
                    "task_success": { "type": "boolean", "description": "Whether the task succeeded with this data" },
                    "value_assessment": { "type": "string", "enum": ["positive", "neutral", "negative"] },
                    "task_type": { "type": "string" },
                    "task_description": { "type": "string" },
                    "comment": { "type": "string" }
                },
                "required": ["cid", "relevance_score", "value_assessment"]
            }),
        },
        ToolDefinition {
            name: "dataset_purchase".into(),
            description: "Purchase a paid dataset using x402 or Machine Payment Protocol. Automatically selects optimal payment protocol.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "cid": { "type": "string" },
                    "max_price": { "type": "number", "description": "Maximum price willing to pay in USD" }
                },
                "required": ["cid"]
            }),
        },
        ToolDefinition {
            name: "dataset_verify".into(),
            description: "Verify dataset integrity and provenance via cryptographic signatures".into(),
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
                "required": ["file_path"]
            }),
        },
        ToolDefinition {
            name: "dataset_reviews".into(),
            description: "List all on-chain feedback/reviews for a dataset (like a product review page). Shows individual reviews and aggregated community signal.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "cid": { "type": "string", "description": "Dataset CID to look up reviews for" }
                },
                "required": ["cid"]
            }),
        },
        ToolDefinition {
            name: "dataset_bt_download".into(),
            description: "Download a dataset from the BitTorrent network by info hash. Use dataset_search with source=bittorrent to find info hashes first.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "info_hash": { "type": "string", "description": "BitTorrent info hash (hex)" }
                },
                "required": ["info_hash"]
            }),
        },
    ]
}
