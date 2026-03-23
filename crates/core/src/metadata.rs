use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::*;

/// Full metadata record for a dataset, stored in DHT and broadcast via GossipSub.
/// Based on Croissant (JSON-LD) with protocol-specific extensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetMetadata {
    // --- Identity ---
    pub cid: DatasetCid,
    pub info_hash: String, // BT v2 info hash hex

    // --- Descriptive ---
    pub title: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub schema: DatasetSchema,
    pub stats: Option<DatasetStats>,

    // --- Access ---
    pub access: AccessMode,
    pub price: Price,
    pub license: License,

    // --- Provenance ---
    pub provider: Did,
    pub signature: String, // Ed25519 signature over canonical JSON
    pub provenance: Provenance,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // --- Optional VC ---
    pub verifiable_credential: Option<serde_json::Value>,
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
