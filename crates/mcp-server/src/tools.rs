// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use serde_json::json;

use crate::protocol::ToolDefinition;

/// Returns all MCP tool definitions exposed by this server.
pub fn all_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "dataset_search".into(),
            description: "Search datasets across registered data skills, including built-in skills such as DefiLlama, RWA.xyz, Guixu Hub, Kaggle, HuggingFace, IPFS, BitTorrent, PostgreSQL, DuckDB, and local files. Supports free open data discovery.".into(),
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
                            "skill_id": {
                                "type": "string",
                                "description": "Optional data skill identifier, e.g. kaggle, huggingface, datacite_commons"
                            },
                            "skill_ids": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Optional allow-list of data skill identifiers"
                            },
                            "source_family": {
                                "type": "string",
                                "enum": [
                                    "marketplace",
                                    "academic",
                                    "web_registry",
                                    "db_catalog",
                                    "decentralized",
                                    "local",
                                    "custom"
                                ]
                            },
                            "source_families": {
                                "type": "array",
                                "items": {
                                    "type": "string",
                                    "enum": [
                                        "marketplace",
                                        "academic",
                                        "web_registry",
                                        "db_catalog",
                                        "decentralized",
                                        "local",
                                        "custom"
                                    ]
                                }
                            },
                            "required_capability": {
                                "type": "string",
                                "enum": [
                                    "search",
                                    "lookup",
                                    "download",
                                    "schema_probe",
                                    "sample_preview",
                                    "license_lookup"
                                ]
                            },
                            "required_capabilities": {
                                "type": "array",
                                "items": {
                                    "type": "string",
                                    "enum": [
                                        "search",
                                        "lookup",
                                        "download",
                                        "schema_probe",
                                        "sample_preview",
                                        "license_lookup"
                                    ]
                                }
                            },
                            "chain": { "type": "string", "description": "Filter by blockchain (e.g. ethereum, polygon)" },
                            "protocol": { "type": "string", "description": "Filter by protocol (e.g. circle, aave)" },
                            "asset": { "type": "string", "description": "Filter by token/asset symbol (e.g. USDC)" },
                            "category": { "type": "string", "description": "Filter by domain (stablecoin, rwa, defi, bridge, yield)" },
                            "free_only": { "type": "boolean", "description": "Only return free/open datasets" }
                        }
                    },
                    "limit": { "type": "integer", "default": 10 }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "dataset_evaluate".into(),
            description: "Compute Task-Conditioned Value (TCV) for a dataset. Returns a score from 0 (harmful) to 100 (highly valuable) based on schema fit, quality, and on-chain community feedback.".into(),
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
            description: "Download a dataset from the BitTorrent network by info hash. Use dataset_search with filters.skill_ids=[\"bittorrent\"] to find info hashes first.".into(),
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
