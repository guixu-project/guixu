use anyhow::Result;
use chrono::Utc;
use data_core::types::*;

/// Trait for external dataset platform adapters.
#[async_trait::async_trait]
pub trait ExternalAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn source_type(&self) -> DataSource;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;
}

// ---------------------------------------------------------------------------
// Kaggle
// ---------------------------------------------------------------------------

pub struct KaggleAdapter {
    pub api_base: String,
}

impl Default for KaggleAdapter {
    fn default() -> Self {
        Self { api_base: "https://www.kaggle.com/api/v1".into() }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for KaggleAdapter {
    fn name(&self) -> &str { "kaggle" }
    fn source_type(&self) -> DataSource { DataSource::Kaggle }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // TODO: Real Kaggle API call. For demo, return mock results
        // matching common dataset patterns.
        let mock = mock_kaggle_results(query, limit);
        Ok(mock)
    }
}

fn mock_kaggle_results(query: &str, limit: usize) -> Vec<SearchResult> {
    let q = query.to_lowercase();
    let mut results = vec![];

    // Simulate Kaggle search with realistic mock data
    if q.contains("gdp") || q.contains("economic") || q.contains("china") {
        results.push(SearchResult {
            cid: DatasetCid("kaggle:sudalairajkumar/world-gdp-data".into()),
            title: "World GDP Data (1960-2025)".into(),
            description: Some("GDP data for all countries from World Bank".into()),
            schema: DatasetSchema {
                columns: vec![
                    ColumnDef { name: "country".into(), dtype: "utf8".into(), nullable: false, description: None },
                    ColumnDef { name: "year".into(), dtype: "int64".into(), nullable: false, description: None },
                    ColumnDef { name: "gdp".into(), dtype: "float64".into(), nullable: true, description: None },
                ],
                row_count: 15000,
                size_bytes: 2_000_000,
            },
            quality: Some(QualityScore {
                total: 78.0, completeness: 85.0, consistency: 80.0,
                freshness: 70.0, schema_quality: 75.0, provenance: 60.0, community: 80.0,
            }),
            price: Price::free(),
            license: License { spdx_id: "CC-BY-4.0".into(), commercial_use: true, derivative_allowed: true },
            provider: Did("did:kaggle:sudalairajkumar".into()),
            source: DataSource::Kaggle,
            created_at: Utc::now(),
        });
    }

    if q.contains("image") || q.contains("classification") || q.contains("medical") {
        results.push(SearchResult {
            cid: DatasetCid("kaggle:nih-chest-xrays/data".into()),
            title: "NIH Chest X-rays".into(),
            description: Some("112,120 X-ray images with disease labels from NIH".into()),
            schema: DatasetSchema {
                columns: vec![
                    ColumnDef { name: "image_path".into(), dtype: "utf8".into(), nullable: false, description: None },
                    ColumnDef { name: "finding_labels".into(), dtype: "utf8".into(), nullable: false, description: None },
                    ColumnDef { name: "patient_id".into(), dtype: "int64".into(), nullable: false, description: None },
                ],
                row_count: 112120,
                size_bytes: 45_000_000_000,
            },
            quality: Some(QualityScore {
                total: 88.0, completeness: 92.0, consistency: 85.0,
                freshness: 60.0, schema_quality: 90.0, provenance: 95.0, community: 88.0,
            }),
            price: Price::free(),
            license: License { spdx_id: "CC0-1.0".into(), commercial_use: true, derivative_allowed: true },
            provider: Did("did:kaggle:nih".into()),
            source: DataSource::Kaggle,
            created_at: Utc::now(),
        });
    }

    results.truncate(limit);
    results
}

// ---------------------------------------------------------------------------
// HuggingFace
// ---------------------------------------------------------------------------

pub struct HuggingFaceAdapter {
    pub api_base: String,
}

impl Default for HuggingFaceAdapter {
    fn default() -> Self {
        Self { api_base: "https://huggingface.co/api".into() }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for HuggingFaceAdapter {
    fn name(&self) -> &str { "huggingface" }
    fn source_type(&self) -> DataSource { DataSource::HuggingFace }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let mock = mock_hf_results(query, limit);
        Ok(mock)
    }
}

fn mock_hf_results(query: &str, limit: usize) -> Vec<SearchResult> {
    let q = query.to_lowercase();
    let mut results = vec![];

    if q.contains("gdp") || q.contains("economic") || q.contains("time series") {
        results.push(SearchResult {
            cid: DatasetCid("hf:worldbank/global-economic-indicators".into()),
            title: "Global Economic Indicators".into(),
            description: Some("Comprehensive economic indicators dataset from World Bank".into()),
            schema: DatasetSchema {
                columns: vec![
                    ColumnDef { name: "country_code".into(), dtype: "utf8".into(), nullable: false, description: None },
                    ColumnDef { name: "indicator".into(), dtype: "utf8".into(), nullable: false, description: None },
                    ColumnDef { name: "year".into(), dtype: "int64".into(), nullable: false, description: None },
                    ColumnDef { name: "value".into(), dtype: "float64".into(), nullable: true, description: None },
                ],
                row_count: 500000,
                size_bytes: 50_000_000,
            },
            quality: Some(QualityScore {
                total: 82.0, completeness: 88.0, consistency: 82.0,
                freshness: 75.0, schema_quality: 80.0, provenance: 85.0, community: 78.0,
            }),
            price: Price::free(),
            license: License { spdx_id: "CC-BY-4.0".into(), commercial_use: true, derivative_allowed: true },
            provider: Did("did:hf:worldbank".into()),
            source: DataSource::HuggingFace,
            created_at: Utc::now(),
        });
    }

    results.truncate(limit);
    results
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
        // IPFS doesn't have native search — in production, we'd query
        // a pinning service index or IPNS name resolution.
        // For demo, return empty (IPFS datasets come via P2P DHT).
        Ok(vec![])
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

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // In production: query information_schema for matching tables
        // For demo: return mock if connection configured
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

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
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
        Box::new(PostgreSqlAdapter::default()),
        Box::new(DuckDbAdapter::default()),
    ]
}
