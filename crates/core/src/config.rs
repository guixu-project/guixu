// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

pub mod migration;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::types::AccessMode;

/// Blockchain network selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockchainNetwork {
    #[default]
    Mainnet,
    Sepolia,
}

impl BlockchainNetwork {
    pub fn chain_id(&self) -> u64 {
        match self {
            Self::Mainnet => 8453,
            Self::Sepolia => 84532,
        }
    }
}

/// Blockchain RPC and API configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainConfig {
    /// Select mainnet or sepolia testnet.
    #[serde(default)]
    pub network: BlockchainNetwork,
    /// Base chain RPC URL.
    #[serde(default = "default_base_rpc")]
    pub base_rpc_url: String,
    /// BaseScan/block explorer API URL.
    #[serde(default = "default_basescan_url")]
    pub basescan_api_url: String,
    /// BaseScan API key (optional).
    #[serde(default)]
    pub basescan_api_key: Option<String>,
    /// x402 payment facilitator URL.
    #[serde(default = "default_x402_facilitator")]
    pub x402_facilitator_url: String,
    /// USDC token contract address on Base.
    #[serde(default = "default_usdc_address")]
    pub usdc_address: String,
    /// USDT token contract address on Base.
    #[serde(default = "default_usdt_address")]
    pub usdt_address: String,
}

fn default_base_rpc() -> String {
    "https://mainnet.base.org".to_string()
}
fn default_basescan_url() -> String {
    "https://api.basescan.org/api".to_string()
}
fn default_x402_facilitator() -> String {
    "https://x402.coinbase.com".to_string()
}
fn default_usdc_address() -> String {
    "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".to_string()
}
fn default_usdt_address() -> String {
    "0xfde4C96c8593536E31F229EA8f37b2ADa2699bb2".to_string()
}

impl Default for BlockchainConfig {
    fn default() -> Self {
        Self {
            network: BlockchainNetwork::Sepolia,
            base_rpc_url: "https://sepolia.base.org".to_string(),
            basescan_api_url: "https://api-sepolia.basescan.org/api".to_string(),
            basescan_api_key: None,
            x402_facilitator_url: default_x402_facilitator(),
            usdc_address: "0x036CbD53842c5426634e7929541eC2318f3dCF7e".to_string(),
            usdt_address: "0x900c915F7E882b8483434b9BA60E86cFb410A597".to_string(),
        }
    }
}

impl BlockchainConfig {
    /// Returns the RPC URL for the current network.
    pub fn rpc_url(&self) -> &str {
        &self.base_rpc_url
    }

    /// Returns the explorer API URL for the current network.
    pub fn explorer_api_url(&self) -> &str {
        &self.basescan_api_url
    }

    /// Returns the USDC address for the current network.
    pub fn usdc(&self) -> &str {
        &self.usdc_address
    }

    /// Returns the USDT address for the current network.
    pub fn usdt(&self) -> &str {
        &self.usdt_address
    }

    /// Returns the chain ID for the current network.
    pub fn chain_id(&self) -> u64 {
        self.network.chain_id()
    }

    /// Returns true if running on testnet.
    pub fn is_testnet(&self) -> bool {
        self.network == BlockchainNetwork::Sepolia
    }
}

/// Server network configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// HTTP/MCP server port.
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    /// P2P listen port.
    #[serde(default = "default_p2p_port")]
    pub p2p_port: u16,
    /// Host to bind to.
    #[serde(default = "default_host")]
    pub host: String,
}

fn default_http_port() -> u16 {
    3927
}
fn default_p2p_port() -> u16 {
    9076
}
fn default_host() -> String {
    "0.0.0.0".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            http_port: default_http_port(),
            p2p_port: default_p2p_port(),
            host: default_host(),
        }
    }
}

/// File system paths configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    /// Config directory (default: ~/.data-node).
    #[serde(default = "default_config_dir")]
    pub config_dir: PathBuf,
    /// Wallet key file path.
    #[serde(default = "default_wallet_key_path")]
    pub wallet_key: PathBuf,
    /// RocksDB database directory.
    #[serde(default = "default_db_path")]
    pub db: PathBuf,
    /// PID file path.
    #[serde(default = "default_pid_path")]
    pub pid_file: PathBuf,
    /// Default data directory for published datasets.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// DuckDB trace database filename.
    #[serde(default = "default_trace_db_name")]
    pub trace_db_name: String,
    /// Download directory.
    #[serde(default = "default_download_dir")]
    pub download_dir: PathBuf,
}

fn default_config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".data-node")
}
fn default_wallet_key_path() -> PathBuf {
    default_config_dir().join("wallet.key")
}
fn default_db_path() -> PathBuf {
    default_config_dir().join("db")
}
fn default_pid_path() -> PathBuf {
    default_config_dir().join("guixu.pid")
}
fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("shared-datasets")
}
fn default_trace_db_name() -> String {
    "traces.duckdb".to_string()
}
fn default_download_dir() -> PathBuf {
    PathBuf::from("downloads")
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            config_dir: default_config_dir(),
            wallet_key: default_wallet_key_path(),
            db: default_db_path(),
            pid_file: default_pid_path(),
            data_dir: default_data_dir(),
            trace_db_name: default_trace_db_name(),
            download_dir: default_download_dir(),
        }
    }
}

/// RocksDB key prefix configuration for all storage stores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePrefixConfig {
    // MetadataStore prefixes
    #[serde(default = "default_meta_prefix")]
    pub meta: String,
    #[serde(default = "default_seed_prefix")]
    pub seed: String,
    #[serde(default = "default_file_prefix")]
    pub file: String,
    #[serde(default = "default_sync_prefix")]
    pub sync: String,
    #[serde(default = "default_access_prefix")]
    pub access: String,
    #[serde(default = "default_hits_prefix")]
    pub hits: String,
    #[serde(default = "default_unpub_prefix")]
    pub unpub: String,
    // JobStore prefixes
    #[serde(default = "default_job_prefix")]
    pub job: String,
    #[serde(default = "default_result_prefix")]
    pub result: String,
    #[serde(default = "default_event_prefix")]
    pub event: String,
    #[serde(default = "default_ingest_prefix")]
    pub ingest: String,
    #[serde(default = "default_download_prefix")]
    pub download: String,
    // FeedbackStore prefix
    #[serde(default = "default_feedback_prefix")]
    pub feedback: String,
    // MemoryStore prefix base
    #[serde(default = "default_memory_prefix")]
    pub memory: String,
}

fn default_meta_prefix() -> String {
    "meta:".to_string()
}
fn default_seed_prefix() -> String {
    "seed:".to_string()
}
fn default_file_prefix() -> String {
    "file:".to_string()
}
fn default_sync_prefix() -> String {
    "sync:".to_string()
}
fn default_access_prefix() -> String {
    "access:".to_string()
}
fn default_hits_prefix() -> String {
    "hits:".to_string()
}
fn default_unpub_prefix() -> String {
    "unpub:".to_string()
}
fn default_job_prefix() -> String {
    "job:".to_string()
}
fn default_result_prefix() -> String {
    "result:".to_string()
}
fn default_event_prefix() -> String {
    "event:".to_string()
}
fn default_ingest_prefix() -> String {
    "ingest:".to_string()
}
fn default_download_prefix() -> String {
    "download:".to_string()
}
fn default_feedback_prefix() -> String {
    "fb:".to_string()
}
fn default_memory_prefix() -> String {
    "mem:".to_string()
}

impl Default for StoragePrefixConfig {
    fn default() -> Self {
        Self {
            meta: default_meta_prefix(),
            seed: default_seed_prefix(),
            file: default_file_prefix(),
            sync: default_sync_prefix(),
            access: default_access_prefix(),
            hits: default_hits_prefix(),
            unpub: default_unpub_prefix(),
            job: default_job_prefix(),
            result: default_result_prefix(),
            event: default_event_prefix(),
            ingest: default_ingest_prefix(),
            download: default_download_prefix(),
            feedback: default_feedback_prefix(),
            memory: default_memory_prefix(),
        }
    }
}

/// Timeouts configuration for various operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutsConfig {
    /// Health check timeout in seconds.
    #[serde(default = "default_health_check_timeout")]
    pub health_check_secs: u64,
    /// HTTP connect timeout in seconds.
    #[serde(default = "default_connect_timeout")]
    pub connect_secs: u64,
    /// HTTP read timeout in seconds.
    #[serde(default = "default_read_timeout")]
    pub read_secs: u64,
    /// Adapter search timeout in seconds.
    #[serde(default = "default_adapter_search_timeout")]
    pub adapter_search_secs: u64,
    /// BitTorrent magnet resolve timeout in seconds.
    #[serde(default = "default_magnet_resolve_timeout")]
    pub magnet_resolve_secs: u64,
    /// Escrow timeout in seconds.
    #[serde(default = "default_escrow_timeout")]
    pub escrow_secs: u64,
}

fn default_health_check_timeout() -> u64 {
    2
}
fn default_connect_timeout() -> u64 {
    15
}
fn default_read_timeout() -> u64 {
    60
}
fn default_adapter_search_timeout() -> u64 {
    5
}
fn default_magnet_resolve_timeout() -> u64 {
    45
}
fn default_escrow_timeout() -> u64 {
    3600
}

impl Default for TimeoutsConfig {
    fn default() -> Self {
        Self {
            health_check_secs: default_health_check_timeout(),
            connect_secs: default_connect_timeout(),
            read_secs: default_read_timeout(),
            adapter_search_secs: default_adapter_search_timeout(),
            magnet_resolve_secs: default_magnet_resolve_timeout(),
            escrow_secs: default_escrow_timeout(),
        }
    }
}

/// Third-party service URLs configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyConfig {
    /// Pan search URL for cloud drive search.
    #[serde(default = "default_pan_search_url")]
    pub pan_search_url: String,
    /// Google dataset search proxy URL.
    #[serde(default = "default_google_search_proxy")]
    pub google_search_proxy: String,
    /// Qdrant vector DB URL.
    #[serde(default = "default_qdrant_url")]
    pub qdrant_url: String,
    /// OTLP collector endpoint.
    #[serde(default = "default_otel_endpoint")]
    pub otlp_endpoint: String,
    /// Market base URL.
    #[serde(default = "default_market_base_url")]
    pub market_base_url: String,
    /// Market public URL.
    #[serde(default = "default_market_public_url")]
    pub market_public_url: String,
    /// BitTorrent tracker announce URL.
    #[serde(default = "default_bt_tracker")]
    pub bt_tracker: String,
}

fn default_pan_search_url() -> String {
    "https://so.252035.xyz".to_string()
}
fn default_google_search_proxy() -> String {
    "https://www.guixu.org/api/search/google".to_string()
}
fn default_qdrant_url() -> String {
    "http://127.0.0.1:6334".to_string()
}
fn default_market_base_url() -> String {
    "http://localhost:8080".to_string()
}
fn default_market_public_url() -> String {
    "https://guixu.org".to_string()
}
fn default_bt_tracker() -> String {
    "https://tracker.opentrackr.org:443/announce".to_string()
}

impl Default for ThirdPartyConfig {
    fn default() -> Self {
        Self {
            pan_search_url: default_pan_search_url(),
            google_search_proxy: default_google_search_proxy(),
            qdrant_url: default_qdrant_url(),
            otlp_endpoint: "http://localhost:4318".to_string(),
            market_base_url: default_market_base_url(),
            market_public_url: default_market_public_url(),
            bt_tracker: default_bt_tracker(),
        }
    }
}

/// Top-level node configuration, persisted at ~/.data-node/config.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// File system paths.
    #[serde(default)]
    pub paths: PathsConfig,
    /// Server network configuration.
    #[serde(default)]
    pub server: ServerConfig,
    /// Storage key prefixes.
    #[serde(default)]
    pub storage_prefixes: StoragePrefixConfig,
    /// Timeouts for various operations.
    #[serde(default)]
    pub timeouts: TimeoutsConfig,
    /// Third-party service URLs.
    #[serde(default)]
    pub third_party: ThirdPartyConfig,
    pub access_default: AccessMode,
    pub price_default: f64,
    pub bootstrap_peers: Vec<String>,
    pub node_mode: NodeMode,
    /// Privacy protection level: "off", "standard", "strict".
    #[serde(default)]
    pub privacy_level: PrivacyLevel,
    /// Differential privacy epsilon (lower = more private).
    #[serde(default = "default_epsilon")]
    pub privacy_epsilon: f64,
    /// Disable mDNS peer discovery (prevents local network IP leak).
    #[serde(default = "default_true")]
    pub disable_mdns: bool,
    /// Use ephemeral DIDs per dataset (prevents cross-dataset correlation).
    #[serde(default)]
    pub ephemeral_dids: bool,
    /// Payment subsystem configuration.
    #[serde(default)]
    pub payment: PaymentConfig,
    /// Adapter names to disable (e.g. ["google_dataset_search", "ipfs", "huggingface"]).
    #[serde(default = "default_disabled_adapters")]
    pub disabled_adapters: Vec<String>,
    /// Enable periodic sync of external data catalogs.
    #[serde(default)]
    pub catalog_sync_enabled: bool,
    /// Refresh interval in seconds (default 3600 = 1h).
    #[serde(default = "default_catalog_sync_interval_secs")]
    pub catalog_sync_interval_secs: u64,
    /// Which sources to sync. Empty = all enabled.
    #[serde(default)]
    pub catalog_sync_sources: Vec<String>,
    /// External DuckDB catalogs for dataset search.
    #[serde(default)]
    pub external_duckdb: Vec<DuckDbCatalog>,
    /// External PostgreSQL catalogs for dataset search.
    #[serde(default)]
    pub external_postgresql: Vec<PostgreSqlCatalog>,
    /// External SQL-over-HTTP catalogs (Spark Thrift, Flink SQL Gateway, Presto/Trino).
    #[serde(default)]
    pub external_sql: Vec<SqlEndpointCatalog>,
    /// Agent trace emission configuration.
    #[serde(default)]
    pub trace: TraceSettings,
    /// Data provider configuration.
    #[serde(default)]
    pub provider: ProviderConfig,
    /// Daemon / watchdog configuration.
    #[serde(default)]
    pub daemon: DaemonConfig,
    /// Network / NAT traversal configuration.
    #[serde(default)]
    pub network: NetworkConfig,
    /// Runtime feature flags for selective enabling of heavy subsystems.
    /// All features default to true for backward compatibility.
    #[serde(default)]
    pub features: FeaturesConfig,
    /// Blockchain RPC, explorer, and token configuration.
    #[serde(default)]
    pub blockchain: BlockchainConfig,
}

/// A DuckDB HTTP server to expose as a searchable catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuckDbCatalog {
    pub label: String,
    /// HTTP URL, e.g. `http://localhost:9999`
    pub url: String,
}

/// A PostgreSQL database to expose as a searchable catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgreSqlCatalog {
    pub label: String,
    pub url: String,
    #[serde(default)]
    pub schemas: Vec<String>,
}

/// A SQL-over-HTTP endpoint (Presto/Trino, Spark Thrift via HTTP, Flink SQL Gateway).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlEndpointCatalog {
    pub label: String,
    /// HTTP base URL, e.g. `http://localhost:8080` for Presto/Trino.
    pub url: String,
    /// Engine type: "presto", "spark", "flink".
    pub engine: SqlEngine,
    /// Optional catalog name (Presto/Trino concept).
    #[serde(default)]
    pub catalog: Option<String>,
    #[serde(default)]
    pub schemas: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SqlEngine {
    Presto,
    Spark,
    Flink,
}

/// Payment subsystem configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentConfig {
    /// Path to the wallet private key file.
    #[serde(default = "default_wallet_path")]
    pub wallet_key_path: PathBuf,
    /// Use testnet (Base Sepolia) instead of mainnet.
    #[serde(default = "default_true")]
    pub testnet: bool,
    /// x402 facilitator URL.
    #[serde(default = "default_facilitator")]
    pub facilitator_url: String,
}

fn default_wallet_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".data-node")
        .join("wallet.key")
}

fn default_facilitator() -> String {
    "https://x402.coinbase.com".into()
}

impl Default for PaymentConfig {
    fn default() -> Self {
        Self {
            wallet_key_path: default_wallet_path(),
            testnet: true,
            facilitator_url: default_facilitator(),
        }
    }
}

impl Default for TraceSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            db_path: default_trace_db_path(),
            buffer_size: default_trace_buffer_size(),
            flush_interval_secs: default_trace_flush_interval(),
            sample_rate: default_trace_sample_rate(),
            auto_export_path: None,
            otel_enabled: false,
            otel_endpoint: default_otel_endpoint(),
            otel_service_name: default_otel_service_name(),
            otel_auth_header: None,
        }
    }
}

fn default_epsilon() -> f64 {
    1.0
}
fn default_true() -> bool {
    true
}
fn default_disabled_adapters() -> Vec<String> {
    vec![
        "google_dataset_search".into(),
        "ipfs".into(),
        "huggingface".into(),
    ]
}

fn default_catalog_sync_interval_secs() -> u64 {
    3600
}

/// Trace emission configuration (disabled by default).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSettings {
    /// Enable active trace emission (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Path to the DuckDB trace database.
    #[serde(default = "default_trace_db_path")]
    pub db_path: String,
    /// Flush when buffer reaches this size.
    #[serde(default = "default_trace_buffer_size")]
    pub buffer_size: usize,
    /// Flush interval in seconds.
    #[serde(default = "default_trace_flush_interval")]
    pub flush_interval_secs: u64,
    /// Sampling rate (0.0 to 1.0).
    #[serde(default = "default_trace_sample_rate")]
    pub sample_rate: f64,
    /// Auto-export path for JSONL traces (optional).
    #[serde(default)]
    pub auto_export_path: Option<String>,
    /// Enable OTLP export using OTel GenAI semantic conventions.
    #[serde(default)]
    pub otel_enabled: bool,
    /// OTLP collector endpoint (e.g. `http://localhost:4318`).
    #[serde(default = "default_otel_endpoint")]
    pub otel_endpoint: String,
    /// Service name for OTel resource attribute.
    #[serde(default = "default_otel_service_name")]
    pub otel_service_name: String,
    /// Optional auth header for OTLP endpoint (e.g. `Bearer <token>`).
    #[serde(default)]
    pub otel_auth_header: Option<String>,
}

fn default_trace_db_path() -> String {
    "traces.duckdb".into()
}

fn default_trace_buffer_size() -> usize {
    100
}

fn default_trace_flush_interval() -> u64 {
    30
}

fn default_trace_sample_rate() -> f64 {
    1.0
}

fn default_otel_endpoint() -> String {
    "http://localhost:4318".into()
}

fn default_otel_service_name() -> String {
    "guixu".into()
}

/// Privacy protection level for metadata publication.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrivacyLevel {
    Off,
    #[default]
    Standard,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeMode {
    Full,
    Light,
}

/// Data provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub auto_publish: bool,
    #[serde(default = "default_open")]
    pub default_access: String,
    #[serde(default)]
    pub default_price: f64,
    #[serde(default = "default_license")]
    pub default_license: String,
    #[serde(default)]
    pub watermark_enabled: bool,
    #[serde(default)]
    pub preview: PreviewConfig,
    #[serde(default)]
    pub seeding: SeedingConfig,
}

fn default_open() -> String {
    "open".into()
}
fn default_license() -> String {
    "CC-BY-4.0".into()
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_publish: true,
            default_access: default_open(),
            default_price: 0.0,
            default_license: default_license(),
            watermark_enabled: false,
            preview: PreviewConfig::default(),
            seeding: SeedingConfig::default(),
        }
    }
}

/// Remote sampling preview configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_max_preview_rows")]
    pub max_preview_rows: u32,
    #[serde(default = "default_max_preview_bytes")]
    pub max_preview_bytes: usize,
    #[serde(default = "default_true")]
    pub paid_schema_preview: bool,
    #[serde(default = "default_true")]
    pub paid_limited_preview: bool,
    #[serde(default = "default_paid_preview_rows")]
    pub paid_preview_rows: u32,
}

fn default_max_preview_rows() -> u32 {
    100
}
fn default_max_preview_bytes() -> usize {
    65536
}
fn default_paid_preview_rows() -> u32 {
    5
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_preview_rows: default_max_preview_rows(),
            max_preview_bytes: default_max_preview_bytes(),
            paid_schema_preview: true,
            paid_limited_preview: true,
            paid_preview_rows: default_paid_preview_rows(),
        }
    }
}

/// BitTorrent seeding configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedingConfig {
    #[serde(default = "default_max_seeds")]
    pub max_seeds: u32,
    #[serde(default)]
    pub upload_rate_limit: u64,
}

fn default_max_seeds() -> u32 {
    50
}

impl Default for SeedingConfig {
    fn default() -> Self {
        Self {
            max_seeds: default_max_seeds(),
            upload_rate_limit: 0,
        }
    }
}

/// Daemon / watchdog configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_watchdog_interval")]
    pub watchdog_interval_secs: u64,
    #[serde(default = "default_watchdog_max_failures")]
    pub watchdog_max_failures: u32,
    #[serde(default = "default_memory_limit")]
    pub memory_limit_mb: u64,
    #[serde(default = "default_disk_min_free")]
    pub disk_min_free_mb: u64,
}

fn default_watchdog_interval() -> u64 {
    30
}
fn default_watchdog_max_failures() -> u32 {
    3
}
fn default_memory_limit() -> u64 {
    2048
}
fn default_disk_min_free() -> u64 {
    100
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            watchdog_interval_secs: default_watchdog_interval(),
            watchdog_max_failures: default_watchdog_max_failures(),
            memory_limit_mb: default_memory_limit(),
            disk_min_free_mb: default_disk_min_free(),
        }
    }
}

/// Network / NAT traversal configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    #[serde(default = "default_true")]
    pub relay_enabled: bool,
    #[serde(default)]
    pub relay_servers: Vec<String>,
    #[serde(default)]
    pub relay_server_enabled: bool,
    #[serde(default = "default_true")]
    pub autonat_enabled: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            relay_enabled: true,
            relay_servers: vec![],
            relay_server_enabled: false,
            autonat_enabled: true,
        }
    }
}

/// Runtime feature flags for selective enabling of heavy subsystems.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeaturesConfig {
    /// Enable blockchain payment (ERC-20/USDC via x402).
    #[serde(default = "default_true")]
    pub blockchain_payment: bool,
    /// Enable on-chain community signal fetching via EAS.
    #[serde(default = "default_true")]
    pub on_chain_signal: bool,
    /// Sentiment classifier for community reviews.
    #[serde(default)]
    pub sentiment_classifier: SentimentClassifierConfig,
    /// Enable BitTorrent seeding and downloading.
    #[serde(default = "default_true")]
    pub p2p_torrent: bool,
    /// Enable DHT network for dataset discovery.
    #[serde(default = "default_true")]
    pub dht_network: bool,
    /// Prioritize open data skill adapters over P2P.
    #[serde(default = "default_true")]
    pub open_data_skills_first: bool,
    /// TCV scoring weights (sum should ≈ 1.0).
    #[serde(default)]
    pub tcv_weights: TcvWeights,
    /// LLM provider for sentiment analysis and fallback scoring.
    #[serde(default)]
    pub llm: LlmConfig,
    /// Explicit adapter blacklist (overrides node_mode defaults).
    #[serde(default)]
    pub adapter_blacklist: Vec<String>,
}

impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            blockchain_payment: true,
            on_chain_signal: true,
            sentiment_classifier: SentimentClassifierConfig::default(),
            p2p_torrent: true,
            dht_network: true,
            open_data_skills_first: true,
            tcv_weights: TcvWeights::default(),
            llm: LlmConfig::default(),
            adapter_blacklist: vec![],
        }
    }
}

/// Sentiment classifier for community review analysis.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SentimentClassifierConfig {
    /// No sentiment classification; use neutral scores.
    None,
    /// Local TF-IDF based classifier (no external API).
    #[default]
    Local,
    /// LLM-based classification using configured provider.
    Llm,
}

/// TCV (Task-Conditioned Value) scoring weights.
/// These weights determine how different factors contribute to dataset ranking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TcvWeights {
    /// Schema match weight (default 0.25).
    #[serde(default = "default_tcv_weight_schema")]
    pub schema_fit: f64,
    /// Temporal relevance weight (default 0.15).
    #[serde(default = "default_tcv_weight_temporal")]
    pub temporal_fit: f64,
    /// Information gain weight (default 0.15).
    #[serde(default = "default_tcv_weight_info")]
    pub info_gain: f64,
    /// Data quality weight (default 0.10).
    #[serde(default = "default_tcv_weight_quality")]
    pub quality: f64,
    /// Community signal weight (default 0.15).
    #[serde(default = "default_tcv_weight_community")]
    pub community_signal: f64,
    /// Risk penalty weight (default 0.20).
    #[serde(default = "default_tcv_weight_risk")]
    pub risk_penalty: f64,
}

impl Default for TcvWeights {
    fn default() -> Self {
        Self {
            schema_fit: default_tcv_weight_schema(),
            temporal_fit: default_tcv_weight_temporal(),
            info_gain: default_tcv_weight_info(),
            quality: default_tcv_weight_quality(),
            community_signal: default_tcv_weight_community(),
            risk_penalty: default_tcv_weight_risk(),
        }
    }
}

fn default_tcv_weight_schema() -> f64 {
    0.25
}
fn default_tcv_weight_temporal() -> f64 {
    0.15
}
fn default_tcv_weight_info() -> f64 {
    0.15
}
fn default_tcv_weight_quality() -> f64 {
    0.10
}
fn default_tcv_weight_community() -> f64 {
    0.15
}
fn default_tcv_weight_risk() -> f64 {
    0.20
}

/// LLM provider configuration for sentiment analysis and fallback scoring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Provider name: "openai", "anthropic", "ollama", "deepseek".
    #[serde(default = "default_llm_provider")]
    pub provider: String,
    /// Model name (provider-specific).
    #[serde(default = "default_llm_model")]
    pub model: String,
    /// API key (or env var reference like "${DEEPSEEK_API_KEY}").
    #[serde(default)]
    pub api_key: Option<String>,
    /// API base URL for custom endpoints.
    #[serde(default)]
    pub api_base: Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: default_llm_provider(),
            model: default_llm_model(),
            api_key: None,
            api_base: None,
        }
    }
}

fn default_llm_provider() -> String {
    "local".to_string()
}

fn default_llm_model() -> String {
    "local".to_string()
}

impl NodeConfig {
    /// Returns the default config directory path.
    /// Use `config.paths.config_dir` for the configured path.
    pub fn config_dir() -> PathBuf {
        default_config_dir()
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    /// Load config from the default path, or return defaults if not found.
    /// Runs migration for backward compatibility (GIP005).
    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        let mut config: Self = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();

        // GIP005: Apply config migration for old installs
        migration::apply_config_migration(&mut config);
        config
    }

    pub fn identity_path() -> PathBuf {
        Self::config_dir().join("identity.key")
    }

    pub fn db_path() -> PathBuf {
        Self::config_dir().join("db")
    }

    pub fn pid_path() -> PathBuf {
        Self::config_dir().join("guixu.pid")
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            paths: PathsConfig::default(),
            server: ServerConfig::default(),
            storage_prefixes: StoragePrefixConfig::default(),
            timeouts: TimeoutsConfig::default(),
            third_party: ThirdPartyConfig::default(),
            access_default: AccessMode::Open,
            price_default: 0.0,
            bootstrap_peers: vec![],
            node_mode: NodeMode::Full,
            privacy_level: PrivacyLevel::Standard,
            privacy_epsilon: 1.0,
            disable_mdns: true,
            ephemeral_dids: false,
            payment: PaymentConfig::default(),
            // Temporarily disabled until guixu.org proxy is ready.
            disabled_adapters: vec![
                "google_dataset_search".into(),
                "ipfs".into(),
                "huggingface".into(),
            ],
            catalog_sync_enabled: false,
            catalog_sync_interval_secs: default_catalog_sync_interval_secs(),
            catalog_sync_sources: vec![],
            external_duckdb: vec![],
            external_postgresql: vec![],
            external_sql: vec![],
            trace: TraceSettings::default(),
            provider: ProviderConfig::default(),
            daemon: DaemonConfig::default(),
            network: NetworkConfig::default(),
            features: FeaturesConfig::default(),
            blockchain: BlockchainConfig::default(),
        }
    }
}
