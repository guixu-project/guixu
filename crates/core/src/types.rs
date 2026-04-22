// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

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
    pub spdx_id: String, // e.g. "CC-BY-4.0"
    pub commercial_use: bool,
    pub derivative_allowed: bool,
}

/// Data type of a dataset — drives type-aware valuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    Tabular,
    Video,
    Image,
    Audio,
    Text,
}

impl DataType {
    /// Infer from file extension.
    pub fn from_ext(ext: &str) -> Self {
        match ext {
            "csv" | "tsv" | "parquet" | "arrow" => Self::Tabular,
            "mp4" | "avi" | "mkv" | "mov" | "webm" => Self::Video,
            "png" | "jpg" | "jpeg" | "webp" | "tiff" => Self::Image,
            "mp3" | "wav" | "flac" | "ogg" => Self::Audio,
            "txt" | "md" | "jsonl" => Self::Text,
            _ => Self::Tabular, // default fallback
        }
    }
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

/// Video-specific metadata for type-aware valuation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMeta {
    pub duration_secs: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub codec: String,
    pub has_audio: bool,
    /// Number of distinct scenes (shot boundary detection).
    pub scene_count: Option<u32>,
    /// Labels / categories if annotated.
    pub labels: Vec<String>,
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
        Self {
            amount: 0.0,
            currency: "USDC".into(),
        }
    }

    pub fn usdc(amount: f64) -> Self {
        Self {
            amount,
            currency: "USDC".into(),
        }
    }

    pub fn is_free(&self) -> bool {
        self.amount == 0.0
    }
}

/// Lightweight marketplace engagement stats associated with a dataset.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatasetMarketStats {
    pub download_count: u64,
    pub review_count: u64,
    pub trade_count: u64,
}

/// A search result returned to the Agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub cid: DatasetCid,
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub schema: DatasetSchema,
    pub quality: Option<QualityScore>,
    pub price: Price,
    pub license: License,
    pub provider: Did,
    pub source: DataSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market: Option<DatasetMarketStats>,
    pub data_type: DataType,
    pub created_at: DateTime<Utc>,
    /// x402-compatible payment endpoint (e.g. from Guixu Hub).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seller_endpoint: Option<String>,
    /// Adapter-specific attributes (chain, protocol, category, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_attributes: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_meta: Option<ProviderMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance: Option<GovernanceMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMeta {
    pub provider_id: String,
    pub source_family: SourceFamily,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFamily {
    Marketplace,
    Academic,
    WebRegistry,
    DbCatalog,
    Decentralized,
    Local,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillCapability {
    Search,
    Lookup,
    Download,
    SchemaProbe,
    SamplePreview,
    LicenseLookup,
    Query,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceMeta {
    pub trust_tier: TrustTier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_hint: Option<RateLimitHint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compliance_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustTier {
    Unknown,
    Community,
    Verified,
    FirstParty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitHint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requests_per_minute: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub burst: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetArtifact {
    pub artifact_id: String,
    pub dataset_cid: DatasetCid,
    pub artifact_type: ArtifactType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    Download,
    Schema,
    Preview,
    Manifest,
    Unknown,
}

/// Where a dataset was discovered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataSource {
    P2p,
    Kaggle,
    HuggingFace,
    Ipfs,
    BitTorrent,
    PostgreSql,
    DuckDb,
    LocalFile,
    GoogleDatasetSearch,
    DataCiteCommons,
    GuixuHub,
    DefiLlama,
    RwaXyz,
    TheGraph,
    PanSearch,
    Dblp,
    SemanticScholar,
    Arxiv,
    Spark,
    Flink,
    Presto,
    OpenDataSkill,
}

/// Payment protocol used for a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Optional response body from the seller endpoint (e.g. download URL from x402).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seller_response: Option<String>,
}

/// Persistent record of a BitTorrent seed managed by this node.
/// Stored in RocksDB under `seed:{info_hash}` so seeds survive restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedRecord {
    pub info_hash: String,
    pub cid: DatasetCid,
    pub file_path: std::path::PathBuf,
    pub access: AccessMode,
    pub title: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
}

/// Request to access a paid dataset via /guixu/access/1.0.0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRequest {
    pub cid: DatasetCid,
    pub buyer_did: Did,
    pub payment_proof: String,
}

/// Grant returned after successful payment verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessGrant {
    pub cid: DatasetCid,
    pub torrent_info_hash: String,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watermark_id: Option<String>,
    #[serde(default)]
    pub watermark_status: String,
    pub granted_at: DateTime<Utc>,
}

/// Notification sent via GossipSub when async watermarking completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatermarkReady {
    pub cid: DatasetCid,
    pub buyer_did: Did,
    pub watermarked_info_hash: String,
}

/// Sample request for /guixu/sample/1.0.0 protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleRequest {
    pub cid: DatasetCid,
    pub max_bytes: usize,
    pub format: String,
    pub rows: usize,
    /// Optional list of column names to include for sparse column selection.
    /// If None, all columns are returned.
    /// For Parquet files this enables efficient column-pruned reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
    /// Optional row filter expression (e.g., "age > 25 AND country == 'US'").
    /// If None, no row filtering is applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_predicate: Option<String>,
}

/// Sample response for /guixu/sample/1.0.0 protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleResponse {
    pub cid: DatasetCid,
    pub schema: DatasetSchema,
    pub preview_data: String,
    pub provider_did: Did,
    pub signature: String,
}

/// Health report from the internal watchdog.
#[derive(Debug, Clone, Default)]
pub struct HealthReport {
    pub p2p_ok: bool,
    pub http_ok: bool,
    pub db_ok: bool,
    pub disk_ok: bool,
    pub memory_ok: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn access_grant_serde_roundtrip() {
        let grant = AccessGrant {
            cid: DatasetCid("cid-test".into()),
            torrent_info_hash: "hash123".into(),
            access_token: "token456".into(),
            watermark_id: Some("wm-abc".into()),
            watermark_status: "pending".into(),
            granted_at: Utc::now(),
        };
        let json = serde_json::to_string(&grant).unwrap();
        let decoded: AccessGrant = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.cid.0, "cid-test");
        assert_eq!(decoded.access_token, "token456");
        assert_eq!(decoded.watermark_id, Some("wm-abc".into()));
    }

    #[test]
    fn access_grant_without_watermark_roundtrip() {
        let grant = AccessGrant {
            cid: DatasetCid("cid-2".into()),
            torrent_info_hash: "h".into(),
            access_token: "t".into(),
            watermark_id: None,
            watermark_status: "none".into(),
            granted_at: Utc::now(),
        };
        let json = serde_json::to_string(&grant).unwrap();
        assert!(!json.contains("watermark_id"));
        let decoded: AccessGrant = serde_json::from_str(&json).unwrap();
        assert!(decoded.watermark_id.is_none());
    }

    #[test]
    fn access_request_serde_roundtrip() {
        let req = AccessRequest {
            cid: DatasetCid("cid-req".into()),
            buyer_did: Did("did:buyer:1".into()),
            payment_proof: "proof123".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: AccessRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.buyer_did.0, "did:buyer:1");
    }

    #[test]
    fn sample_request_serde_roundtrip() {
        let req = SampleRequest {
            cid: DatasetCid("cid-sr".into()),
            max_bytes: 65536,
            format: "head".into(),
            rows: 20,
            columns: None,
            row_predicate: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: SampleRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_bytes, 65536);
        assert_eq!(decoded.rows, 20);
    }

    #[test]
    fn sample_response_serde_roundtrip() {
        let resp = SampleResponse {
            cid: DatasetCid("cid-resp".into()),
            schema: DatasetSchema {
                columns: vec![ColumnDef {
                    name: "col1".into(),
                    dtype: "utf8".into(),
                    nullable: true,
                    description: None,
                }],
                row_count: 100,
                size_bytes: 2048,
            },
            preview_data: "aGVsbG8=".into(),
            provider_did: Did("did:provider:1".into()),
            signature: "sig123".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: SampleResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.schema.row_count, 100);
        assert_eq!(decoded.schema.columns.len(), 1);
    }

    #[test]
    fn watermark_ready_serde_roundtrip() {
        let wr = WatermarkReady {
            cid: DatasetCid("cid-wr".into()),
            buyer_did: Did("did:buyer:wr".into()),
            watermarked_info_hash: "wmhash".into(),
        };
        let json = serde_json::to_string(&wr).unwrap();
        let decoded: WatermarkReady = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.watermarked_info_hash, "wmhash");
    }

    #[test]
    fn seed_record_serde_roundtrip() {
        let sr = SeedRecord {
            info_hash: "ih123".into(),
            cid: DatasetCid("cid-sr".into()),
            file_path: "/tmp/data.csv".into(),
            access: AccessMode::Paid,
            title: "test seed".into(),
            size_bytes: 4096,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&sr).unwrap();
        let decoded: SeedRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.info_hash, "ih123");
        assert_eq!(decoded.access, AccessMode::Paid);
    }
}
