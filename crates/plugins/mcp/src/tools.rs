// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

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
            description: "Parse a data request into a structured QueryProfile. You MUST extract task_type, keywords, sample_unit, and target_entity from the user's request. keywords should contain ONLY dataset content terms (e.g. 'cat', 'lung nodule', 'chest ct'), NOT task/action words (e.g. 'classification', 'detection', 'build'). Maximum 5 keywords. Call this BEFORE dataset_search.".into(),
            annotations: read_only_annotations(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The user's original data request verbatim"
                    },
                    "task_type": {
                        "type": "string",
                        "enum": ["classification", "detection", "segmentation", "forecasting", "ranking", "retrieval", "generation", "summarization", "evaluation"],
                        "description": "The type of ML/data task"
                    },
                    "task_description": {
                        "type": "string",
                        "description": "Detailed description of what the user wants to accomplish with the data"
                    },
                    "target_entity": {
                        "type": "string",
                        "description": "Main subject of the dataset (e.g. 'cat', 'lung nodule', 'stock price')"
                    },
                    "keywords": {
                        "type": "array",
                        "items": { "type": "string" },
                        "maxItems": 5,
                        "description": "Dataset content search terms only. NO task words like 'classifier' or 'detection'."
                    },
                    "sample_unit": {
                        "type": "string",
                        "enum": ["image", "video", "text", "tabular", "audio", ""],
                        "description": "Data modality. Empty string if unknown."
                    },
                    "budget": {
                        "type": "string",
                        "description": "Budget with currency, e.g. '20 USD', '$50', '0.05 ETH'. Use '0 USD' if none."
                    }
                },
                "required": ["query", "task_type", "keywords", "sample_unit"]
            }),
        },
        ToolDefinition {
            name: "dataset_search".into(),
            description: "Search datasets across DefiLlama, RWA.xyz, Kaggle, HuggingFace, IPFS, BitTorrent, DBLP, Semantic Scholar, arXiv, and P2P network. Supports free open data discovery.".into(),
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
                            "source": {
                                "type": "string",
                                "enum": [
                                    "defillama", "rwa_xyz", "thegraph",
                                    "guixuhub", "kaggle", "huggingface",
                                    "ipfs", "bittorrent", "postgresql",
                                    "duckdb", "googledatasetsearch",
                                    "datacitecommons", "pansearch", "p2p",
                                    "dblp", "semanticscholar", "arxiv"
                                ]
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
            name: "dataset_download".into(),
            description: "Download a dataset by CID. Automatically selects the right method based on source. Free no-login sources: UCI (uci:), OpenML (openml:), Zenodo (zenodo:), Figshare (figshare:), Common Crawl (commoncrawl:), OpenAlex (openalex:), AWS Open Data (aws-open:), OpenNeuro (openneuro:), PhysioNet (physionet:), HuggingFace public (hf:), IPFS (ipfs:), Guixu Hub free (guixu-hub:), BitTorrent (40-char hex hash). Requires login: Kaggle (kaggle:). Pass the CID from dataset_search results.".into(),
            annotations: local_side_effect_annotations(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "cid": {
                        "type": "string",
                        "description": "Dataset CID from search results (e.g. 'kaggle:owner/dataset', 'hf:owner/dataset', 'uci:53', 'openml:61', 'zenodo:12345', 'figshare:12345', 'guixu-hub:uuid')"
                    }
                },
                "required": ["cid"]
            }),
        },
        ToolDefinition {
            name: "dataset_lookup".into(),
            description: "Lookup detailed dataset metadata by CID, including schema, license, provider, and stored source attributes.".into(),
            annotations: read_only_annotations(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "cid": { "type": "string", "description": "Dataset CID from search results" }
                },
                "required": ["cid"]
            }),
        },
        ToolDefinition {
            name: "dataset_schema_probe".into(),
            description: "Return dataset schema and related type information by CID.".into(),
            annotations: read_only_annotations(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "cid": { "type": "string", "description": "Dataset CID from search results" }
                },
                "required": ["cid"]
            }),
        },
        ToolDefinition {
            name: "dataset_download_by_skill".into(),
            description: "Download a dataset through the Open Data Skill execution path. Useful for skill-backed providers and future declarative downloads.".into(),
            annotations: local_side_effect_annotations(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "cid": { "type": "string", "description": "Dataset CID from search results" }
                },
                "required": ["cid"]
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
        ToolDefinition {
            name: "pan_search".into(),
            description: "Search public cloud-drive share links (Baidu, Quark, Aliyun, 115, OneDrive) across aggregated indexes. Returns share URLs with access codes. Only accesses publicly shared content.".into(),
            annotations: read_only_annotations(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search keywords (e.g. movie/TV show name)" },
                    "platform": {
                        "type": "string",
                        "enum": ["baidu", "quark", "aliyun", "xunlei", "onedrive", "115"],
                        "description": "Optional: filter by cloud-drive platform"
                    },
                    "limit": { "type": "integer", "default": 20, "description": "Max results to return" }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "data_task_delegate".into(),
            description: "Delegate a dataset discovery, valuation, and acquisition task to the Guixu Agent. The agent will search, evaluate, and select the best dataset based on the task specification. Returns a job_id for tracking.".into(),
            annotations: mutating_annotations(false),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "host_kind": {
                        "type": "string",
                        "enum": ["openclaw", "codex", "opencode"],
                        "description": "The host agent type"
                    },
                    "session_key": {
                        "type": "string",
                        "description": "Host session key for context"
                    },
                    "run_id": {
                        "type": "string",
                        "description": "Optional run identifier"
                    },
                    "workspace_id": {
                        "type": "string",
                        "description": "Workspace identifier"
                    },
                    "workspace_root": {
                        "type": "string",
                        "description": "Optional workspace root path"
                    },
                    "goal": {
                        "type": "string",
                        "description": "What the user wants to accomplish (e.g. 'train a cat detector')"
                    },
                    "task_type": {
                        "type": "string",
                        "description": "Optional ML task type (classification, detection, etc.)"
                    },
                    "required_modalities": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Data modalities needed (image, video, text, tabular, audio)"
                    },
                    "required_columns": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Required dataset columns"
                    },
                    "budget_amount": {
                        "type": "number",
                        "description": "Budget amount"
                    },
                    "budget_currency": {
                        "type": "string",
                        "default": "USD",
                        "description": "Budget currency"
                    },
                    "allow_purchase": {
                        "type": "boolean",
                        "default": false,
                        "description": "Whether to allow purchasing paid datasets"
                    },
                    "allowed_sources": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Allowed data sources (kaggle, huggingface, ipfs, etc.)"
                    },
                    "require_license_review": {
                        "type": "boolean",
                        "default": true,
                        "description": "Whether to require license review before download"
                    },
                    "desired_outputs": {
                        "type": "array",
                        "items": { "type": "string" },
                        "default": ["selected_dataset"],
                        "description": "What outputs to produce (selected_dataset, evaluation_report, downloaded_artifact, guixu_lock)"
                    }
                },
                "required": ["host_kind", "session_key", "workspace_id", "goal"]
            }),
        },
        ToolDefinition {
            name: "data_task_status".into(),
            description: "Get the status of a delegated data task. Returns current state, selected dataset if completed, and any errors.".into(),
            annotations: read_only_annotations(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "job_id": {
                        "type": "string",
                        "description": "The job ID returned by data_task_delegate"
                    }
                },
                "required": ["job_id"]
            }),
        },
        ToolDefinition {
            name: "data_task_approve".into(),
            description: "Approve or reject a pending action (purchase, publish, credential use) for a delegated task.".into(),
            annotations: mutating_annotations(true),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "job_id": {
                        "type": "string",
                        "description": "The job ID"
                    },
                    "action": {
                        "type": "string",
                        "enum": ["purchase", "publish", "override_policy"],
                        "description": "The action type being approved"
                    },
                    "approved": {
                        "type": "boolean",
                        "description": "Whether to approve (true) or reject (false)"
                    },
                    "notes": {
                        "type": "string",
                        "description": "Optional notes explaining the decision"
                    }
                },
                "required": ["job_id", "action", "approved"]
            }),
        },
        ToolDefinition {
            name: "data_task_cancel".into(),
            description: "Cancel a running or queued delegated task.".into(),
            annotations: mutating_annotations(true),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "job_id": {
                        "type": "string",
                        "description": "The job ID to cancel"
                    },
                    "reason": {
                        "type": "string",
                        "description": "Optional reason for cancellation"
                    }
                },
                "required": ["job_id"]
            }),
        },
        ToolDefinition {
            name: "data_task_artifacts".into(),
            description: "Get the artifacts produced by a completed delegated task.".into(),
            annotations: read_only_annotations(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "job_id": {
                        "type": "string",
                        "description": "The job ID"
                    }
                },
                "required": ["job_id"]
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

    #[test]
    fn data_task_delegate_is_exposed() {
        let tool_names: Vec<String> = all_tool_definitions()
            .into_iter()
            .map(|tool| tool.name)
            .collect();

        assert!(tool_names.iter().any(|name| name == "data_task_delegate"));
    }
}
