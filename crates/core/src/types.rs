use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Content identifier for a dataset (content-addressed hash).
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct DatasetCid(pub String);

/// Decentralized identifier for a node/user.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Did(pub String);

/// BitTorrent v2 info hash (Merkle root of pieces).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoHash(pub [u8; 32]);

/// Access mode for a dataset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessMode {
    /// Free, open swarm — anyone can seed.
    Open,
    /// Paid, seller-only seeding — buyer cannot re-distribute.
    Paid,
}

/// License type (machine-readable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    pub spdx_id: String,       // e.g. "CC-BY-4.0"
    pub commercial_use: bool,
    pub derivative_allowed: bool,
}

/// Column definition in a dataset schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub dtype: String, // "int64", "float64", "utf8", "date", ...
    pub nullable: bool,
    pub description: Option<String>,
}

/// Schema of a dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSchema {
    pub columns: Vec<ColumnDef>,
    pub row_count: u64,
    pub size_bytes: u64,
}

/// Statistical summary embedded in metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetStats {
    pub null_rate: f64,
    pub unique_rate: f64,
    pub min_values: serde_json::Value,
    pub max_values: serde_json::Value,
}

/// Quality score (0-100) with breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityScore {
    pub total: f64,
    pub completeness: f64,
    pub consistency: f64,
    pub freshness: f64,
    pub schema_quality: f64,
    pub provenance: f64,
    pub community: f64,
}

/// Price in USDC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Price {
    pub amount: f64,
    pub currency: String,
}

impl Price {
    pub fn free() -> Self {
        Self { amount: 0.0, currency: "USDC".into() }
    }

    pub fn usdc(amount: f64) -> Self {
        Self { amount, currency: "USDC".into() }
    }

    pub fn is_free(&self) -> bool {
        self.amount == 0.0
    }
}

/// A search result returned to the Agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub cid: DatasetCid,
    pub title: String,
    pub description: Option<String>,
    pub schema: DatasetSchema,
    pub quality: Option<QualityScore>,
    pub price: Price,
    pub license: License,
    pub provider: Did,
    pub source: DataSource,
    pub created_at: DateTime<Utc>,
}

/// Where a dataset was discovered.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataSource {
    P2p,
    Kaggle,
    HuggingFace,
    DataGov,
    Ipfs,
    BitTorrent,
    PostgreSql,
    DuckDb,
}

/// Payment protocol used for a transaction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentProtocol {
    X402,
    StripeMpp,
    Erc8183,
}

/// Receipt of a completed transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionReceipt {
    pub tx_id: String,
    pub buyer: Did,
    pub seller: Did,
    pub dataset_cid: DatasetCid,
    pub price: Price,
    pub protocol: PaymentProtocol,
    pub timestamp: DateTime<Utc>,
}
