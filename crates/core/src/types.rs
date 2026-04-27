// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

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
    Subscribe,
    Backfill,
    Decode,
    Simulate,
    Execute,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_class: Option<LatencyClass>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freshness_sla_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reorg_safety_level: Option<ReorgSafetyLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmations_required: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decode_confidence: Option<DecodeConfidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustTier {
    Unknown,
    Community,
    Verified,
    FirstParty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LatencyClass {
    Hot,
    Warm,
    Cold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReorgSafetyLevel {
    ZeroConfirmations,
    OneConfirmation,
    TwoConfirmations,
    SixConfirmations,
    TwelveConfirmations,
    Finalized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecodeConfidence {
    High,
    Medium,
    Low,
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

// ============================================================================
// Delivery Manifest & Ingest Job Types
// ============================================================================

/// Reference to a single artifact within a delivery manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub artifact_id: String,
    pub protocol: String, // "https" | "bt" | "ipfs" | "file"
    pub uri: String,
    pub size_bytes: u64,
    pub checksum: Option<String>,
    pub supports_range: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_headers: Option<std::collections::HashMap<String, String>>,
}

/// Packaging metadata for a delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackagingInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>, // e.g. "tar+parquet", "zip"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<String>, // e.g. "zstd", "gzip"
}

/// Access constraints for a delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_limit: Option<u32>,
}

/// Delivery manifest issued by guixu.market after order fulfillment.
/// This is the authoritative record of what artifacts are available,
/// how they are packaged, and how they can be accessed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryManifest {
    pub order_id: String,
    pub delivery_id: String,
    pub dataset_id: String,
    pub artifacts: Vec<ArtifactRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packaging: Option<PackagingInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access: Option<AccessInfo>,
}

/// State of an ingest job that manages large file download and processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestState {
    Pending,
    Downloading,
    Verifying,
    Extracting,
    Completed,
    Failed,
    Cancelled,
}

/// A download and ingestion job managed by the ingest subsystem.
/// Tracks the full lifecycle of a large file acquisition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestJob {
    pub job_id: uuid::Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    pub dataset_id: String,
    pub manifest: DeliveryManifest,
    pub state: IngestState,
    pub target_bytes: u64,
    pub downloaded_bytes: u64,
    pub verified_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

// ============================================================================
// Confidential Valuation Types
// ============================================================================

/// Proof verification verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofVerdict {
    Verified,
    Failed,
    Unsupported,
    Pending,
}

/// Evidence mode for valuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValuationEvidenceMode {
    PlaintextSample,
    ZkCommittedSummary,
    FheTaskScoring,
    MetadataOnly,
}

/// Confidential valuation evidence attached to a proxy utility report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidentialValuationEvidence {
    pub report_id: String,
    pub report_digest: String,
    pub manifest_digest: Option<String>,
    pub dataset_commitment_digest: Option<String>,
    pub proof_system: String,
    pub proof_verdict: ProofVerdict,
    pub score: Option<f64>,
    pub score_band: Option<String>,
    pub recommendation: Option<String>,
    pub risk_flags: Vec<String>,
    pub roi_band: Option<String>,
}

// ============================================================================
// Signal Event Model Types (Real-time Signal Discovery)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalFamily {
    Mempool,
    Swap,
    Bridge,
    Mint,
    Governance,
    ContractVerify,
    WhaleFlow,
    NewPair,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxHash(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSource {
    pub skill_id: String,
    pub adapter_kind: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityRefs {
    #[serde(default)]
    pub tokens: Vec<String>,
    #[serde(default)]
    pub pools: Vec<String>,
    #[serde(default)]
    pub wallets: Vec<String>,
    #[serde(default)]
    pub contracts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub evidence_type: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalEvent {
    pub signal_id: SignalId,
    pub signal_family: SignalFamily,
    pub chain_id: ChainId,
    pub block_number: u64,
    pub tx_hash: TxHash,
    pub observed_at: DateTime<Utc>,
    pub source: SignalSource,
    pub entity_refs: EntityRefs,
    #[serde(default)]
    pub features: HashMap<String, f64>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    pub confidence: f64,
    pub freshness_ms: u64,
    pub reorg_safe: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Pool,
    Vault,
    Bridge,
    Minter,
    Deployer,
    LP,
    MarketMaker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketEntity {
    pub entity_type: EntityType,
    pub chain_id: ChainId,
    pub address: Address,
    #[serde(default)]
    pub labels: Vec<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlphaScore {
    pub freshness_score: f64,
    pub novelty_score: f64,
    pub entity_importance_score: f64,
    pub flow_score: f64,
    pub execution_score: f64,
    pub risk_score: f64,
    pub evidence_score: f64,
    pub decay_score: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionAction {
    Alert,
    Simulate,
    SemiAuto,
    AutoExecute,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionIntent {
    pub action: ExecutionAction,
    #[serde(default)]
    pub route: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gas_budget: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slippage_bps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opportunity {
    pub opportunity_id: Uuid,
    #[serde(default)]
    pub signal_events: Vec<SignalEvent>,
    pub alpha_score: AlphaScore,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_plan: Option<ExecutionIntent>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}
