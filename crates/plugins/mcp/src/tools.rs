use crate::protocol::{ToolAnnotations, ToolDefinition};
use serde_json::json;

fn read_only_annotations() -> Option<ToolAnnotations> {
    Some(ToolAnnotations {
        read_only_hint: Some(true),
        destructive_hint: Some(false),
        idempotent_hint: Some(true),
        open_world_hint: Some(true),
    })
}

fn mutating_annotations(open_world: bool) -> Option<ToolAnnotations> {
    Some(ToolAnnotations {
        read_only_hint: Some(false),
        destructive_hint: Some(false),
        idempotent_hint: Some(false),
        open_world_hint: Some(open_world),
    })
}

fn local_side_effect_annotations() -> Option<ToolAnnotations> {
    Some(ToolAnnotations {
        read_only_hint: Some(false),
        destructive_hint: Some(false),
        idempotent_hint: Some(false),
        open_world_hint: Some(true),
    })
}

/// Returns all MCP tool definitions exposed by this server.
pub fn all_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "intent_parse".into(),
            description: "Parse a natural-language task into a structured QueryProfile for inspection or debugging. Pass the user's original request verbatim in raw_query when available; do not paraphrase it first.".into(),
            annotations: read_only_annotations(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Backward-compatible query field. If raw_query is present, this may contain an agent-side working rewrite."
                    },
                    "raw_query": {
                        "type": "string",
                        "description": "The user's original request verbatim. Prefer this field for intent parsing."
                    }
                },
                "anyOf": [
                    { "required": ["query"] },
                    { "required": ["raw_query"] }
                ]
            }),
        },
        ToolDefinition {
            name: "dataset_search".into(),
            description: "Search datasets across Kaggle, HuggingFace, IPFS, BitTorrent, PostgreSQL, DuckDB and P2P network".into(),
            annotations: read_only_annotations(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural language search query" },
                    "task_type": {
                        "type": "string",
                        "description": "Optional task category used to filter and rank compatible dataset modalities"
                    },
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
            annotations: read_only_annotations(),
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
                    "budget": {
                        "anyOf": [
                            { "type": "number" },
                            { "type": "string" }
                        ],
                        "description": "Maximum budget, preserving unit/currency when available, e.g. $20 or 20 USD"
                    }
                },
                "required": ["cid", "task_description"]
            }),
        },
        ToolDefinition {
            name: "dataset_feedback".into(),
            description: "Submit on-chain feedback after using a dataset. Recorded as an EAS attestation to help future agents evaluate this dataset.".into(),
            annotations: mutating_annotations(false),
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
            annotations: mutating_annotations(true),
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
            annotations: read_only_annotations(),
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
            annotations: mutating_annotations(true),
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
            annotations: read_only_annotations(),
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
            annotations: local_side_effect_annotations(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "info_hash": { "type": "string", "description": "BitTorrent info hash (hex)" }
                },
                "required": ["info_hash"]
            }),
        },
        ToolDefinition {
            name: "dataset_bt_preview".into(),
            description: "Download a partial preview of a BitTorrent dataset (first N bytes) without downloading the full file.".into(),
            annotations: local_side_effect_annotations(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "info_hash": { "type": "string", "description": "BitTorrent info hash (hex)" },
                    "max_bytes": { "type": "integer", "description": "Maximum bytes to preview (default 65536)", "default": 65536 }
                },
                "required": ["info_hash"]
            }),
        },
        ToolDefinition {
            name: "dataset_bt_stats".into(),
            description: "Get download progress and speed for an active BitTorrent download.".into(),
            annotations: read_only_annotations(),
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

pub fn validate_tool_definitions(
    definitions: &[ToolDefinition],
) -> std::result::Result<(), String> {
    for definition in definitions {
        let valid = !definition.name.is_empty()
            && definition
                .name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
        if !valid {
            return Err(format!(
                "invalid MCP tool name '{}': only letters, digits, '-', '_' and '.' are allowed",
                definition.name
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::all_tool_definitions;

    #[test]
    fn task_pipeline_is_not_exposed() {
        let tool_names: Vec<String> = all_tool_definitions()
            .into_iter()
            .map(|tool| tool.name)
            .collect();

        assert!(tool_names.iter().any(|name| name == "intent_parse"));
        assert!(tool_names.iter().any(|name| name == "dataset_search"));
        assert!(tool_names.iter().any(|name| name == "dataset_evaluate"));
        assert!(!tool_names.iter().any(|name| name == "task_pipeline"));
    }
}
