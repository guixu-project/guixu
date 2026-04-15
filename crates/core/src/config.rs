// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::types::AccessMode;

/// Top-level node configuration, persisted at ~/.data-node/config.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub data_dir: PathBuf,
    pub access_default: AccessMode,
    pub price_default: f64,
    pub listen_port: u16,
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

impl NodeConfig {
    pub fn config_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".data-node")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn identity_path() -> PathBuf {
        Self::config_dir().join("identity.key")
    }

    pub fn db_path() -> PathBuf {
        Self::config_dir().join("db")
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            data_dir: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("shared-datasets"),
            access_default: AccessMode::Open,
            price_default: 0.0,
            listen_port: 9076,
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
        }
    }
}
