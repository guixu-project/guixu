use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;
use tracing::{warn, debug};

/// Trait for external dataset platform adapters.
#[async_trait::async_trait]
pub trait ExternalAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn source_type(&self) -> DataSource;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;
}

// ---------------------------------------------------------------------------
// Kaggle — requires KAGGLE_USERNAME + KAGGLE_KEY env vars
// ---------------------------------------------------------------------------

pub struct KaggleAdapter {
    pub api_base: String,
    enabled: bool,
}

impl Default for KaggleAdapter {
    fn default() -> Self {
        let enabled = std::env::var("KAGGLE_USERNAME").is_ok()
            && std::env::var("KAGGLE_KEY").is_ok();
        Self {
            api_base: "https://www.kaggle.com/api/v1".into(),
            enabled,
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for KaggleAdapter {
    fn name(&self) -> &str { "kaggle" }
    fn source_type(&self) -> DataSource { DataSource::Kaggle }

    async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SearchResult>> {
        if !self.enabled {
            return Ok(vec![]);
        }
        // TODO: Real Kaggle API call using KAGGLE_USERNAME/KAGGLE_KEY
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// HuggingFace — requires HF_TOKEN env var
// ---------------------------------------------------------------------------

pub struct HuggingFaceAdapter {
    pub api_base: String,
    enabled: bool,
}

impl Default for HuggingFaceAdapter {
    fn default() -> Self {
        let enabled = std::env::var("HF_TOKEN").is_ok();
        Self {
            api_base: "https://huggingface.co/api".into(),
            enabled,
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for HuggingFaceAdapter {
    fn name(&self) -> &str { "huggingface" }
    fn source_type(&self) -> DataSource { DataSource::HuggingFace }

    async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SearchResult>> {
        if !self.enabled {
            return Ok(vec![]);
        }
        // TODO: Real HuggingFace Datasets API call using HF_TOKEN
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// IPFS
// ---------------------------------------------------------------------------

pub struct IpfsAdapter {
    pub gateway_url: String,
}

impl Default for IpfsAdapter {
    fn default() -> Self {
        Self { gateway_url: "https://ipfs.io".into() }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for IpfsAdapter {
    fn name(&self) -> &str { "ipfs" }
    fn source_type(&self) -> DataSource { DataSource::Ipfs }

    async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SearchResult>> {
        // IPFS doesn't have native search — datasets come via P2P DHT.
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// BitTorrent — search via bitsearch.to public API
// ---------------------------------------------------------------------------

pub struct BitTorrentAdapter {
    client: reqwest::Client,
    api_base: String,
}

impl Default for BitTorrentAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(3))
                .timeout(Duration::from_secs(8))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_base: "https://bitsearch.to/api/v1".into(),
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for BitTorrentAdapter {
    fn name(&self) -> &str { "bittorrent" }
    fn source_type(&self) -> DataSource { DataSource::BitTorrent }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let url = format!("{}/search", self.api_base);
        debug!(adapter = "bittorrent", %url, %query, "sending search request");

        let resp = match tokio::time::timeout(
            Duration::from_secs(10),
            self.client
                .get(&url)
                .query(&[
                    ("q", query),
                    ("limit", &limit.min(100).to_string()),
                    ("sort", "seeders"),
                ])
                .send(),
        )
        .await
        {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                warn!(adapter = "bittorrent", error = %e, "HTTP request failed");
                return Err(e.into());
            }
            Err(_) => {
                warn!(adapter = "bittorrent", "request timed out after 10s");
                return Err(anyhow!("bittorrent search timed out after 10s"));
            }
        };

        let status = resp.status();
        if !status.is_success() {
            warn!(adapter = "bittorrent", %status, "non-success HTTP status");
            return Err(anyhow!("bittorrent API returned {status}"));
        }

        let resp = resp.json::<serde_json::Value>().await.map_err(|e| {
            warn!(adapter = "bittorrent", error = %e, "failed to parse JSON response");
            anyhow!("bittorrent JSON parse error: {e}")
        })?;

        let results = resp
            .get("results")
            .and_then(|v| v.as_array())
            .cloned()
            .ok_or_else(|| anyhow!("bitsearch response missing 'results' array"))?;

        Ok(results
            .into_iter()
            .filter_map(|item| {
                let infohash = item.get("infohash")?.as_str()?;
                let title = item.get("title")?.as_str()?;
                let size = item.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                let seeders = item.get("seeders").and_then(|v| v.as_u64()).unwrap_or(0);
                let created = item
                    .get("createdAt")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);

                Some(SearchResult {
                    cid: DatasetCid(infohash.to_string()),
                    title: title.to_string(),
                    description: Some(format!("{seeders} seeders")),
                    schema: DatasetSchema {
                        columns: vec![],
                        row_count: 0,
                        size_bytes: size,
                    },
                    quality: None,
                    price: Price::free(),
                    license: License {
                        spdx_id: "unknown".into(),
                        commercial_use: false,
                        derivative_allowed: false,
                    },
                    provider: Did(format!("bt:{infohash}")),
                    source: DataSource::BitTorrent,
                    created_at: created,
                })
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Database adapters (PostgreSQL, DuckDB)
// ---------------------------------------------------------------------------

pub struct PostgreSqlAdapter {
    pub connection_string: Option<String>,
}

impl Default for PostgreSqlAdapter {
    fn default() -> Self {
        Self { connection_string: None }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for PostgreSqlAdapter {
    fn name(&self) -> &str { "postgresql" }
    fn source_type(&self) -> DataSource { DataSource::PostgreSql }

    async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SearchResult>> {
        if self.connection_string.is_none() {
            return Ok(vec![]);
        }
        // TODO: Real PostgreSQL information_schema query
        Ok(vec![])
    }
}

pub struct DuckDbAdapter {
    pub db_path: Option<String>,
}

impl Default for DuckDbAdapter {
    fn default() -> Self {
        Self { db_path: None }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for DuckDbAdapter {
    fn name(&self) -> &str { "duckdb" }
    fn source_type(&self) -> DataSource { DataSource::DuckDb }

    async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SearchResult>> {
        if self.db_path.is_none() {
            return Ok(vec![]);
        }
        // TODO: Real DuckDB catalog query
        Ok(vec![])
    }
}

/// Create all default adapters.
pub fn default_adapters() -> Vec<Box<dyn ExternalAdapter>> {
    vec![
        Box::new(KaggleAdapter::default()),
        Box::new(HuggingFaceAdapter::default()),
        Box::new(IpfsAdapter::default()),
        Box::new(BitTorrentAdapter::default()),
        Box::new(PostgreSqlAdapter::default()),
        Box::new(DuckDbAdapter::default()),
    ]
}
