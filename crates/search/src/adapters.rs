use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;
use tracing::{debug, warn};

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
        let enabled =
            std::env::var("KAGGLE_USERNAME").is_ok() && std::env::var("KAGGLE_KEY").is_ok();
        Self {
            api_base: "https://www.kaggle.com/api/v1".into(),
            enabled,
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for KaggleAdapter {
    fn name(&self) -> &str {
        "kaggle"
    }
    fn source_type(&self) -> DataSource {
        DataSource::Kaggle
    }

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
    fn name(&self) -> &str {
        "huggingface"
    }
    fn source_type(&self) -> DataSource {
        DataSource::HuggingFace
    }

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
        Self {
            gateway_url: "https://ipfs.io".into(),
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for IpfsAdapter {
    fn name(&self) -> &str {
        "ipfs"
    }
    fn source_type(&self) -> DataSource {
        DataSource::Ipfs
    }

    async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SearchResult>> {
        // IPFS doesn't have native search — datasets come via P2P DHT.
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// BitTorrent — multi-source search with fallback
//   1. apibay.org  (The Pirate Bay public API — most reliable)
//   2. bitsearch.to
//   3. solidtorrents.to
// ---------------------------------------------------------------------------

pub struct BitTorrentAdapter {
    client: reqwest::Client,
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
        }
    }
}

impl BitTorrentAdapter {
    /// Try a single HTTP GET, return parsed JSON or error.
    async fn try_get(&self, url: &str, query: &[(&str, &str)]) -> Result<serde_json::Value> {
        let resp = tokio::time::timeout(
            Duration::from_secs(8),
            self.client.get(url).query(query).send(),
        )
        .await
        .map_err(|_| anyhow!("timeout"))??
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
        Ok(resp)
    }

    /// apibay.org — returns JSON array directly.
    async fn search_apibay(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let resp = self
            .try_get("https://apibay.org/q.php", &[("q", query), ("cat", "0")])
            .await?;

        let items = resp
            .as_array()
            .ok_or_else(|| anyhow!("apibay: expected array"))?;

        // apibay returns [{"id":"0","name":"No results",...}] for no results
        if items.len() == 1 {
            if let Some(name) = items[0].get("name").and_then(|v| v.as_str()) {
                if name == "No results returned" {
                    return Ok(vec![]);
                }
            }
        }

        Ok(items
            .iter()
            .take(limit)
            .filter_map(Self::parse_apibay)
            .collect())
    }

    fn parse_apibay(item: &serde_json::Value) -> Option<SearchResult> {
        let hash = item.get("info_hash")?.as_str()?;
        let name = item.get("name")?.as_str()?;
        let size = item
            .get("size")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let seeders = item
            .get("seeders")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let added = item
            .get("added")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let created = chrono::DateTime::from_timestamp(added, 0).unwrap_or_else(chrono::Utc::now);

        Some(SearchResult {
            cid: DatasetCid(hash.to_string()),
            title: name.to_string(),
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
            provider: Did(format!("bt:{hash}")),
            source: DataSource::BitTorrent,
            data_type: infer_data_type_from_title(name),
            created_at: created,
        })
    }

    /// bitsearch.to — returns { results: [...] }.
    async fn search_bitsearch(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let limit_str = limit.min(100).to_string();
        let resp = self
            .try_get(
                "https://bitsearch.to/api/v1/search",
                &[("q", query), ("limit", &limit_str), ("sort", "seeders")],
            )
            .await?;

        let items = resp
            .get("results")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("bitsearch: missing 'results'"))?;

        Ok(items
            .iter()
            .take(limit)
            .filter_map(Self::parse_bitsearch)
            .collect())
    }

    fn parse_bitsearch(item: &serde_json::Value) -> Option<SearchResult> {
        let hash = item.get("infohash")?.as_str()?;
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
            cid: DatasetCid(hash.to_string()),
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
            provider: Did(format!("bt:{hash}")),
            source: DataSource::BitTorrent,
            data_type: infer_data_type_from_title(title),
            created_at: created,
        })
    }

    /// solidtorrents.to — returns { results: [...] }.
    async fn search_solidtorrents(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let resp = self
            .try_get(
                "https://solidtorrents.to/api/v1/search",
                &[("q", query), ("sort", "seeders")],
            )
            .await?;

        let items = resp
            .get("results")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("solidtorrents: missing 'results'"))?;

        Ok(items
            .iter()
            .take(limit)
            .filter_map(|item| {
                let hash = item.get("infohash").or_else(|| item.get("_id"))?.as_str()?;
                let title = item.get("title")?.as_str()?;
                let size = item.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                let seeders = item
                    .pointer("/swarm/seeders")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                Some(SearchResult {
                    cid: DatasetCid(hash.to_string()),
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
                    provider: Did(format!("bt:{hash}")),
                    source: DataSource::BitTorrent,
                    data_type: infer_data_type_from_title(title),
                    created_at: chrono::Utc::now(),
                })
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for BitTorrentAdapter {
    fn name(&self) -> &str {
        "bittorrent"
    }
    fn source_type(&self) -> DataSource {
        DataSource::BitTorrent
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // Try sources in order; return first success.
        match self.search_apibay(query, limit).await {
            Ok(r) => {
                debug!(
                    adapter = "bittorrent",
                    source = "apibay",
                    count = r.len(),
                    "ok"
                );
                return Ok(r);
            }
            Err(e) => {
                warn!(adapter = "bittorrent", source = "apibay", error = %e, "failed, trying next")
            }
        }
        match self.search_bitsearch(query, limit).await {
            Ok(r) => {
                debug!(
                    adapter = "bittorrent",
                    source = "bitsearch",
                    count = r.len(),
                    "ok"
                );
                return Ok(r);
            }
            Err(e) => {
                warn!(adapter = "bittorrent", source = "bitsearch", error = %e, "failed, trying next")
            }
        }
        match self.search_solidtorrents(query, limit).await {
            Ok(r) => {
                debug!(
                    adapter = "bittorrent",
                    source = "solidtorrents",
                    count = r.len(),
                    "ok"
                );
                return Ok(r);
            }
            Err(e) => {
                warn!(adapter = "bittorrent", source = "solidtorrents", error = %e, "all sources failed")
            }
        }
        Err(anyhow!("all BitTorrent search sources failed"))
    }
}

// ---------------------------------------------------------------------------
// Database adapters (PostgreSQL, DuckDB)
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct PostgreSqlAdapter {
    pub connection_string: Option<String>,
}

#[async_trait::async_trait]
impl ExternalAdapter for PostgreSqlAdapter {
    fn name(&self) -> &str {
        "postgresql"
    }
    fn source_type(&self) -> DataSource {
        DataSource::PostgreSql
    }

    async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SearchResult>> {
        if self.connection_string.is_none() {
            return Ok(vec![]);
        }
        // TODO: Real PostgreSQL information_schema query
        Ok(vec![])
    }
}

#[derive(Default)]
pub struct DuckDbAdapter {
    pub db_path: Option<String>,
}

#[async_trait::async_trait]
impl ExternalAdapter for DuckDbAdapter {
    fn name(&self) -> &str {
        "duckdb"
    }
    fn source_type(&self) -> DataSource {
        DataSource::DuckDb
    }

    async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SearchResult>> {
        if self.db_path.is_none() {
            return Ok(vec![]);
        }
        // TODO: Real DuckDB catalog query
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// Google Dataset Search — via Google Custom Search JSON API
//   Requires GOOGLE_API_KEY and GOOGLE_CSE_ID env vars.
//   Create a Programmable Search Engine scoped to datasetsearch.research.google.com
//   at https://programmablesearchengine.google.com/
// ---------------------------------------------------------------------------

pub struct GoogleDatasetSearchAdapter {
    client: reqwest::Client,
    api_key: Option<String>,
    cse_id: Option<String>,
}

impl Default for GoogleDatasetSearchAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_key: std::env::var("GOOGLE_API_KEY").ok(),
            cse_id: std::env::var("GOOGLE_CSE_ID").ok(),
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for GoogleDatasetSearchAdapter {
    fn name(&self) -> &str {
        "google_dataset_search"
    }
    fn source_type(&self) -> DataSource {
        DataSource::GoogleDatasetSearch
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let (api_key, cse_id) = match (&self.api_key, &self.cse_id) {
            (Some(k), Some(c)) => (k.as_str(), c.as_str()),
            _ => return Ok(vec![]),
        };

        let num = limit.min(10).to_string(); // Google CSE max 10 per request
        let resp = tokio::time::timeout(
            Duration::from_secs(10),
            self.client
                .get("https://www.googleapis.com/customsearch/v1")
                .query(&[
                    ("key", api_key),
                    ("cx", cse_id),
                    ("q", query),
                    ("num", &num),
                ])
                .send(),
        )
        .await
        .map_err(|_| anyhow!("google dataset search: timeout"))??
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

        let empty = vec![];
        let items = resp
            .get("items")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        Ok(items
            .iter()
            .take(limit)
            .filter_map(|item| {
                let title = item.get("title").and_then(|v| v.as_str())?;
                let link = item.get("link").and_then(|v| v.as_str()).unwrap_or("");
                let snippet = item
                    .get("snippet")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let cid_hash = {
                    use sha2::Digest;
                    let mut h = sha2::Sha256::new();
                    h.update(format!("gds:{title}:{link}").as_bytes());
                    hex::encode(h.finalize())
                };

                Some(SearchResult {
                    cid: DatasetCid(cid_hash),
                    title: title.to_string(),
                    description: snippet,
                    schema: DatasetSchema {
                        columns: vec![],
                        row_count: 0,
                        size_bytes: 0,
                    },
                    quality: None,
                    price: Price::free(),
                    license: License {
                        spdx_id: "unknown".into(),
                        commercial_use: false,
                        derivative_allowed: false,
                    },
                    provider: Did(format!("gds:{link}")),
                    source: DataSource::GoogleDatasetSearch,
                    data_type: infer_data_type_from_title(title),
                    created_at: chrono::Utc::now(),
                })
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// DataCite Commons — uses the public DataCite REST API
// ---------------------------------------------------------------------------

pub struct DataCiteCommonsAdapter {
    client: reqwest::Client,
}

impl Default for DataCiteCommonsAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for DataCiteCommonsAdapter {
    fn name(&self) -> &str {
        "datacite_commons"
    }
    fn source_type(&self) -> DataSource {
        DataSource::DataCiteCommons
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let page_size = limit.min(25).to_string();
        let resp = tokio::time::timeout(
            Duration::from_secs(10),
            self.client
                .get("https://api.datacite.org/dois")
                .query(&[
                    ("query", query),
                    ("resource-type-id", "dataset"),
                    ("page[size]", &page_size),
                ])
                .send(),
        )
        .await
        .map_err(|_| anyhow!("datacite: timeout"))??
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

        let empty = vec![];
        let items = resp
            .get("data")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        Ok(items
            .iter()
            .take(limit)
            .filter_map(|item| {
                let attrs = item.get("attributes")?;
                let doi = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let title = attrs
                    .get("titles")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .and_then(|t| t.get("title"))
                    .and_then(|v| v.as_str())?;
                let desc = attrs
                    .get("descriptions")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .and_then(|d| d.get("description"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let year = attrs
                    .get("publicationYear")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let created = chrono::DateTime::parse_from_rfc3339(
                    attrs
                        .get("registered")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                )
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

                Some(SearchResult {
                    cid: DatasetCid(doi.to_string()),
                    title: title.to_string(),
                    description: desc.map(|d| if year > 0 { format!("[{year}] {d}") } else { d }),
                    schema: DatasetSchema {
                        columns: vec![],
                        row_count: 0,
                        size_bytes: 0,
                    },
                    quality: None,
                    price: Price::free(),
                    license: License {
                        spdx_id: "unknown".into(),
                        commercial_use: false,
                        derivative_allowed: false,
                    },
                    provider: Did(format!("doi:{doi}")),
                    source: DataSource::DataCiteCommons,
                    data_type: infer_data_type_from_title(title),
                    created_at: created,
                })
            })
            .collect())
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
        Box::new(LocalFileAdapter::default()),
        Box::new(GoogleDatasetSearchAdapter::default()),
        Box::new(DataCiteCommonsAdapter::default()),
    ]
}

// ---------------------------------------------------------------------------
// Local file adapter (Parquet / CSV / JSON / TSV)
// ---------------------------------------------------------------------------

/// Scans user-specified directories for data files and matches by filename /
/// column names against the search query.  Supports Parquet, CSV, TSV and JSON.
pub struct LocalFileAdapter {
    /// Directories to scan. Empty → adapter is a no-op.
    pub dirs: Vec<std::path::PathBuf>,
}

impl Default for LocalFileAdapter {
    fn default() -> Self {
        // Honour GUIXU_DATA_DIRS env (colon-separated) if set
        let dirs = std::env::var("GUIXU_DATA_DIRS")
            .unwrap_or_default()
            .split(':')
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .collect();
        Self { dirs }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for LocalFileAdapter {
    fn name(&self) -> &str {
        "local_file"
    }
    fn source_type(&self) -> DataSource {
        DataSource::LocalFile
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if self.dirs.is_empty() {
            return Ok(vec![]);
        }

        let query_lower = query.to_lowercase();
        let keywords: Vec<&str> = query_lower.split_whitespace().collect();
        let mut results = Vec::new();

        for dir in &self.dirs {
            for ext in &["csv", "tsv", "parquet", "json", "ndjson"] {
                let pattern = format!("{}/**/*.{ext}", dir.display());
                let paths = glob::glob(&pattern).unwrap_or_else(|_| glob::glob("").unwrap());
                for entry in paths.flatten() {
                    if results.len() >= limit {
                        break;
                    }
                    if let Some(r) = Self::probe_file(&entry, &query_lower, &keywords) {
                        results.push(r);
                    }
                }
            }
        }

        Ok(results)
    }
}

impl LocalFileAdapter {
    fn probe_file(path: &std::path::Path, query: &str, keywords: &[&str]) -> Option<SearchResult> {
        let file_name = path.file_stem()?.to_str()?.to_lowercase();
        let ext = path.extension()?.to_str()?;

        let (columns, row_count) = match ext {
            "parquet" => Self::read_parquet_schema(path).ok()?,
            "csv" | "tsv" => Self::read_csv_schema(path, ext == "tsv").ok()?,
            "json" | "ndjson" => Self::read_json_schema(path).ok()?,
            _ => return None,
        };

        let col_text = columns
            .iter()
            .map(|c| c.name.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        let all_text = format!("{file_name} {col_text}");

        // Match: any keyword appears in filename or column names
        let matched = all_text.contains(query) || keywords.iter().any(|kw| all_text.contains(kw));
        if !matched {
            return None;
        }

        let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let cid_hash = {
            use sha2::Digest;
            let mut h = sha2::Sha256::new();
            h.update(path.to_string_lossy().as_bytes());
            hex::encode(h.finalize())
        };

        Some(SearchResult {
            cid: DatasetCid(cid_hash),
            title: path.file_name()?.to_str()?.to_string(),
            description: Some(format!("local file: {}", path.display())),
            schema: DatasetSchema {
                columns,
                row_count,
                size_bytes,
            },
            quality: None,
            price: Price::free(),
            license: License {
                spdx_id: "proprietary".into(),
                commercial_use: false,
                derivative_allowed: false,
            },
            provider: Did("did:local:self".into()),
            source: DataSource::LocalFile,
            data_type: DataType::from_ext(ext),
            created_at: chrono::Utc::now(),
        })
    }

    fn read_parquet_schema(path: &std::path::Path) -> Result<(Vec<ColumnDef>, u64)> {
        use polars::prelude::*;
        let file = std::fs::File::open(path)?;
        let reader = ParquetReader::new(file);
        let df = reader.finish()?;
        let columns = df
            .get_columns()
            .iter()
            .map(|s| ColumnDef {
                name: s.name().to_string(),
                dtype: format!("{}", s.dtype()),
                nullable: true,
                description: None,
            })
            .collect();
        let row_count = df.height() as u64;
        Ok((columns, row_count))
    }

    fn read_csv_schema(path: &std::path::Path, is_tsv: bool) -> Result<(Vec<ColumnDef>, u64)> {
        use polars::prelude::*;
        let sep = if is_tsv { b'\t' } else { b',' };
        let df = CsvReadOptions::default()
            .with_parse_options(CsvParseOptions::default().with_separator(sep))
            .with_n_rows(Some(256)) // only peek
            .try_into_reader_with_file_path(Some(path.into()))?
            .finish()?;
        let columns = df
            .get_columns()
            .iter()
            .map(|s| ColumnDef {
                name: s.name().to_string(),
                dtype: format!("{}", s.dtype()),
                nullable: true,
                description: None,
            })
            .collect();
        let row_count = df.height() as u64;
        Ok((columns, row_count))
    }

    fn read_json_schema(path: &std::path::Path) -> Result<(Vec<ColumnDef>, u64)> {
        use polars::prelude::*;
        let file = std::fs::File::open(path)?;
        let df = JsonReader::new(file).finish()?;
        let columns = df
            .get_columns()
            .iter()
            .map(|s| ColumnDef {
                name: s.name().to_string(),
                dtype: format!("{}", s.dtype()),
                nullable: true,
                description: None,
            })
            .collect();
        let row_count = df.height() as u64;
        Ok((columns, row_count))
    }
}

/// Infer data type from a torrent/file title by scanning for known extensions
/// and content keywords.
pub(crate) fn infer_data_type_from_title(title: &str) -> DataType {
    let t = title.to_lowercase();
    // Tabular data extensions
    for ext in [".csv", ".tsv", ".parquet", ".arrow", ".xlsx", ".xls"] {
        if t.contains(ext) {
            return DataType::Tabular;
        }
    }
    // Video extensions + encoding hints
    for kw in [
        ".mp4", ".avi", ".mkv", ".mov", ".webm", ".ts", "x264", "x265", "hevc", "h264", "h265",
        "avc", "1080p", "720p", "2160p", "4k", "bluray", "bdrip", "webrip", "web-dl", "hdtv",
        "dvdrip", "remux",
    ] {
        if t.contains(kw) {
            return DataType::Video;
        }
    }
    // Image
    for kw in [
        ".png",
        ".jpg",
        ".jpeg",
        ".webp",
        ".tiff",
        ".bmp",
        ".raw",
        "imagenet",
        "coco dataset",
        "photos",
    ] {
        if t.contains(kw) {
            return DataType::Image;
        }
    }
    // Audio
    for kw in [
        ".mp3",
        ".wav",
        ".flac",
        ".ogg",
        ".aac",
        ".m4a",
        "audiobook",
        "podcast",
        "lossless",
    ] {
        if t.contains(kw) {
            return DataType::Audio;
        }
    }
    // Text
    for kw in [
        ".txt", ".md", ".jsonl", ".json", ".pdf", ".epub", ".doc", ".docx", ".ndjson",
    ] {
        if t.contains(kw) {
            return DataType::Text;
        }
    }
    // Tabular keyword hints
    if t.contains("dataset") || t.contains("database") {
        return DataType::Tabular;
    }
    // Season/episode patterns → video
    if t.contains(" s0")
        || t.contains(" s1")
        || t.contains(" s2")
        || t.contains("season")
        || t.contains("episode")
    {
        return DataType::Video;
    }
    DataType::Tabular
}
