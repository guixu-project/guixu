// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::*;

/// Full metadata record for a dataset, stored in DHT and broadcast via GossipSub.
/// Based on Croissant (JSON-LD) with protocol-specific extensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetMetadata {
    // --- Identity ---
    pub cid: DatasetCid,
    #[serde(default)]
    pub info_hash: Option<String>, // BT v2 info hash hex (None for external catalog sources)

    // --- Descriptive ---
    pub title: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub data_type: DataType,
    pub schema: DatasetSchema,
    pub stats: Option<DatasetStats>,
    pub video_meta: Option<VideoMeta>,

    // --- Access ---
    pub access: AccessMode,
    pub price: Price,
    pub license: License,

    // --- Version chain for dataset versioning ---
    /// Semantic version string (e.g., "1.0.0", "2.1.3"). None for initial version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// CID of the previous version this was derived from. Forms a version chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_version: Option<DatasetCid>,

    // --- Provenance ---
    pub provider: Did,
    pub signature: String, // Ed25519 signature over canonical JSON
    pub provenance: Provenance,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // --- Optional VC ---
    pub verifiable_credential: Option<serde_json::Value>,

    /// Adapter-specific domain attributes (mirrors SearchResult.source_attributes)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_attributes: Option<serde_json::Value>,
}

/// How this dataset was produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    Original,
    Derived { sources: Vec<DatasetCid> },
    Aggregated { sources: Vec<DatasetCid> },
}

impl DatasetMetadata {
    /// Canonical JSON bytes for signing (deterministic field order).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut m = self.clone();
        m.signature = String::new(); // exclude signature itself
        serde_json::to_vec(&m).unwrap_or_default()
    }
}
