// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use data_core::config::{DuckDbCatalog, PostgreSqlCatalog, SqlEndpointCatalog};
use data_core::types::*;
use serde::Deserialize;

use super::util::infer_data_type_from_title;
use super::{
    ArxivAdapter, BitTorrentAdapter, DataCiteCommonsAdapter, DblpAdapter, DefiLlamaAdapter,
    DuckDbAdapter, ExternalAdapter, GoogleDatasetSearchAdapter, GuixuHubAdapter,
    HuggingFaceAdapter, IpfsAdapter, KaggleAdapter, LocalFileAdapter, PanSearchAdapter,
    PostgreSqlAdapter, RwaXyzAdapter, SemanticScholarAdapter, SqlEndpointAdapter,
};

const BUILTIN_SKILL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/skills/builtin");

#[derive(Debug, Clone, Deserialize)]
pub struct OpenDataSkillSpec {
    #[serde(default = "default_spec_version")]
    pub spec_version: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub capabilities: SkillCapabilities,
    #[serde(default)]
    pub governance: SkillGovernance,
    pub provider: SkillProvider,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillCapabilities {
    #[serde(default = "default_true")]
    pub search: bool,
    #[serde(default)]
    pub lookup: bool,
    #[serde(default)]
    pub download: bool,
    #[serde(default)]
    pub schema_probe: bool,
    #[serde(default)]
    pub sample_preview: bool,
    #[serde(default)]
    pub license_lookup: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillGovernance {
    #[serde(default = "default_trust_tier")]
    pub trust_tier: TrustTier,
    #[serde(default)]
    pub rate_limit_hint: Option<RateLimitHint>,
    #[serde(default)]
    pub provenance_hint: Option<String>,
    #[serde(default)]
    pub compliance_hint: Option<String>,
}

impl Default for SkillGovernance {
    fn default() -> Self {
        Self {
            trust_tier: TrustTier::Unknown,
            rate_limit_hint: None,
            provenance_hint: None,
            compliance_hint: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillProvider {
    NativeAdapter {
        adapter: String,
    },
    HttpSearch {
        base_url: String,
        #[serde(default)]
        operations: SkillOperations,
        #[serde(default)]
        item_mapping: SkillItemMapping,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillOperations {
    pub search: HttpOperation,
    #[serde(default)]
    pub lookup: Option<HttpOperation>,
    #[serde(default)]
    pub download: Option<HttpOperation>,
    #[serde(default)]
    pub schema_probe: Option<HttpOperation>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HttpOperation {
    pub path: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub query_param: String,
    #[serde(default)]
    pub limit_param: Option<String>,
    #[serde(default)]
    pub static_params: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub headers: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub auth: SkillAuth,
    #[serde(default)]
    pub result_path: Option<String>,
    #[serde(default)]
    pub pagination: PaginationConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillAuth {
    #[default]
    None,
    BearerEnv {
        env: String,
    },
    HeaderEnv {
        header: String,
        env: String,
    },
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PaginationConfig {
    #[default]
    None,
    OffsetLimit {
        page_param: String,
        size_param: String,
        start: usize,
        step: usize,
        max_pages: usize,
    },
    PageNumber {
        page_param: String,
        size_param: String,
        start: usize,
        max_pages: usize,
    },
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillItemMapping {
    #[serde(default = "default_title_field")]
    pub title: String,
    #[serde(default = "default_description_field")]
    pub description: String,
    #[serde(default = "default_id_field")]
    pub id: String,
    #[serde(default)]
    pub size_bytes: Option<String>,
}

fn default_method() -> String {
    "GET".into()
}

fn default_true() -> bool {
    true
}

fn default_trust_tier() -> TrustTier {
    TrustTier::Unknown
}

fn default_spec_version() -> String {
    "1.0".into()
}

fn default_title_field() -> String {
    "title".into()
}

fn default_description_field() -> String {
    "description".into()
}

fn default_id_field() -> String {
    "id".into()
}

pub fn load_open_data_skills() -> Result<Vec<OpenDataSkillSpec>> {
    let mut skills = Vec::new();

    for dir in skill_dirs() {
        if !dir.exists() {
            continue;
        }
        for entry in
            fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let is_json = path.extension().and_then(|ext| ext.to_str()) == Some("json");
            if !is_json {
                continue;
            }
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("failed to read skill file {}", path.display()))?;
            let skill: OpenDataSkillSpec = serde_json::from_str(&raw)
                .with_context(|| format!("invalid open data skill {}", path.display()))?;
            validate_skill_spec(&skill)
                .with_context(|| format!("invalid open data skill {}", path.display()))?;
            if skill.enabled {
                skills.push(skill);
            }
        }
    }

    Ok(skills)
}

fn skill_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from(BUILTIN_SKILL_DIR)];
    if let Ok(extra) = std::env::var("GUIXU_OPEN_DATA_SKILL_DIRS") {
        dirs.extend(
            extra
                .split(':')
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        );
    }
    dirs
}

pub fn adapters_from_open_data_skills(
    disabled: &[String],
    duckdb_catalogs: &[DuckDbCatalog],
    pg_catalogs: &[PostgreSqlCatalog],
    sql_catalogs: &[SqlEndpointCatalog],
) -> Vec<Box<dyn ExternalAdapter>> {
    let skills = load_open_data_skills().unwrap_or_default();
    skills
        .into_iter()
        .filter(|skill| !disabled.iter().any(|d| d.eq_ignore_ascii_case(&skill.id)))
        .filter_map(|skill| {
            build_adapter_from_skill(&skill, duckdb_catalogs, pg_catalogs, sql_catalogs)
                .map_err(|error| {
                    tracing::warn!(skill = %skill.id, error = %error, "failed to load open data skill")
                })
                .ok()
        })
        .collect()
}

fn build_adapter_from_skill(
    skill: &OpenDataSkillSpec,
    duckdb_catalogs: &[DuckDbCatalog],
    pg_catalogs: &[PostgreSqlCatalog],
    sql_catalogs: &[SqlEndpointCatalog],
) -> Result<Box<dyn ExternalAdapter>> {
    match &skill.provider {
        SkillProvider::NativeAdapter { adapter } => {
            native_adapter_from_name(adapter, duckdb_catalogs, pg_catalogs, sql_catalogs)
        }
        SkillProvider::HttpSearch {
            base_url,
            operations,
            item_mapping,
        } => Ok(Box::new(HttpSkillAdapter::new(HttpSkillAdapterConfig {
            id: skill.id.clone(),
            name: skill.name.clone(),
            description: skill.description.clone(),
            source: parse_skill_source(&skill.source)?,
            source_family: infer_source_family(&skill.source),
            tags: skill.tags.clone(),
            base_url: base_url.clone(),
            capabilities: skill.capabilities.clone(),
            governance: skill.governance.clone(),
            operations: operations.clone(),
            item_mapping: item_mapping.clone(),
        }))),
    }
}

fn validate_skill_spec(skill: &OpenDataSkillSpec) -> Result<()> {
    if !skill.spec_version.starts_with("1.") {
        return Err(anyhow!(
            "unsupported open data skill spec_version: {}",
            skill.spec_version
        ));
    }
    if skill.id.trim().is_empty() {
        return Err(anyhow!("skill id must not be empty"));
    }
    if let SkillProvider::HttpSearch { operations, .. } = &skill.provider {
        if operations.search.path.trim().is_empty() {
            return Err(anyhow!(
                "http_search operation.search.path must not be empty"
            ));
        }
    }
    Ok(())
}

fn native_adapter_from_name(
    adapter: &str,
    duckdb_catalogs: &[DuckDbCatalog],
    pg_catalogs: &[PostgreSqlCatalog],
    sql_catalogs: &[SqlEndpointCatalog],
) -> Result<Box<dyn ExternalAdapter>> {
    Ok(match adapter {
        "kaggle" => Box::new(KaggleAdapter::default()),
        "huggingface" => Box::new(HuggingFaceAdapter::default()),
        "ipfs" => Box::new(IpfsAdapter::default()),
        "guixu_hub" => Box::new(GuixuHubAdapter::default()),
        "bittorrent" => Box::new(BitTorrentAdapter::default()),
        "postgresql" => Box::new(PostgreSqlAdapter::with_catalogs(pg_catalogs.to_vec())),
        "duckdb" => Box::new(DuckDbAdapter::with_catalogs(duckdb_catalogs.to_vec())),
        "sql_endpoint" => Box::new(SqlEndpointAdapter::with_catalogs(sql_catalogs.to_vec())),
        "local_file" => Box::new(LocalFileAdapter::default()),
        "google_dataset_search" => Box::new(GoogleDatasetSearchAdapter::default()),
        "datacite_commons" => Box::new(DataCiteCommonsAdapter::default()),
        "defillama" => Box::new(DefiLlamaAdapter::default()),
        "rwa_xyz" => Box::new(RwaXyzAdapter::default()),
        "pan_search" => Box::new(PanSearchAdapter::default()),
        "dblp" => Box::new(DblpAdapter::default()),
        "semantic_scholar" => Box::new(SemanticScholarAdapter::default()),
        "arxiv" => Box::new(ArxivAdapter::default()),
        other => return Err(anyhow!("unknown native adapter: {other}")),
    })
}

fn parse_skill_source(source: &str) -> Result<DataSource> {
    Ok(match source.to_ascii_lowercase().as_str() {
        "kaggle" => DataSource::Kaggle,
        "huggingface" => DataSource::HuggingFace,
        "ipfs" => DataSource::Ipfs,
        "bittorrent" => DataSource::BitTorrent,
        "postgresql" => DataSource::PostgreSql,
        "duckdb" => DataSource::DuckDb,
        "localfile" | "local_file" => DataSource::LocalFile,
        "googledatasetsearch" | "google_dataset_search" => DataSource::GoogleDatasetSearch,
        "datacitecommons" | "datacite_commons" => DataSource::DataCiteCommons,
        "guixuhub" | "guixu_hub" | "guixu-hub" => DataSource::GuixuHub,
        "defillama" => DataSource::DefiLlama,
        "rwa_xyz" | "rwaxyz" => DataSource::RwaXyz,
        "pansearch" | "pan_search" => DataSource::PanSearch,
        "dblp" => DataSource::Dblp,
        "semanticscholar" | "semantic_scholar" => DataSource::SemanticScholar,
        "arxiv" => DataSource::Arxiv,
        "spark" => DataSource::Spark,
        "flink" => DataSource::Flink,
        "presto" => DataSource::Presto,
        "opendataskill" | "open_data_skill" => DataSource::OpenDataSkill,
        other => return Err(anyhow!("unknown data source in skill: {other}")),
    })
}

struct HttpSkillAdapterConfig {
    id: String,
    name: String,
    description: String,
    source: DataSource,
    source_family: SourceFamily,
    tags: Vec<String>,
    base_url: String,
    capabilities: SkillCapabilities,
    governance: SkillGovernance,
    operations: SkillOperations,
    item_mapping: SkillItemMapping,
}

struct HttpSkillAdapter {
    config: HttpSkillAdapterConfig,
    client: reqwest::Client,
}

impl HttpSkillAdapter {
    fn new(config: HttpSkillAdapterConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    fn endpoint_url(&self) -> String {
        format!(
            "{}{}",
            self.config.base_url.trim_end_matches('/'),
            self.config.operations.search.path
        )
    }

    fn value_at_path<'a>(
        value: &'a serde_json::Value,
        path: &str,
    ) -> Option<&'a serde_json::Value> {
        if path.is_empty() {
            return Some(value);
        }
        let mut current = value;
        for part in path.split('.') {
            current = current.get(part)?;
        }
        Some(current)
    }

    fn base_headers(&self, operation: &HttpOperation) -> Result<reqwest::header::HeaderMap> {
        let mut headers = reqwest::header::HeaderMap::new();
        for (key, value) in &operation.headers {
            let header_name = reqwest::header::HeaderName::from_bytes(key.as_bytes())?;
            let header_value = reqwest::header::HeaderValue::from_str(
                value.as_str().unwrap_or(&value.to_string()),
            )?;
            headers.insert(header_name, header_value);
        }

        match &operation.auth {
            SkillAuth::None => {}
            SkillAuth::BearerEnv { env } => {
                if let Ok(token) = std::env::var(env) {
                    let value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))?;
                    headers.insert(reqwest::header::AUTHORIZATION, value);
                }
            }
            SkillAuth::HeaderEnv { header, env } => {
                if let Ok(token) = std::env::var(env) {
                    let header_name = reqwest::header::HeaderName::from_bytes(header.as_bytes())?;
                    let header_value = reqwest::header::HeaderValue::from_str(&token)?;
                    headers.insert(header_name, header_value);
                }
            }
        }

        Ok(headers)
    }

    async fn execute_request(
        &self,
        operation: &HttpOperation,
        params: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let headers = self.base_headers(operation)?;
        let method = operation.method.to_ascii_uppercase();
        let response = if method == "POST" {
            self.client
                .post(format!(
                    "{}{}",
                    self.config.base_url.trim_end_matches('/'),
                    operation.path
                ))
                .headers(headers)
                .json(params)
                .send()
                .await?
        } else {
            let query_params: Vec<(String, String)> = params
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or(&v.to_string()).to_string()))
                .collect();
            self.client
                .get(format!(
                    "{}{}",
                    self.config.base_url.trim_end_matches('/'),
                    operation.path
                ))
                .headers(headers)
                .query(&query_params)
                .send()
                .await?
        }
        .error_for_status()?;

        Ok(response.json::<serde_json::Value>().await?)
    }

    async fn fetch_page(
        &self,
        operation: &HttpOperation,
        query: &str,
        limit: usize,
        page_index: usize,
    ) -> Result<Vec<serde_json::Value>> {
        let mut params = operation.static_params.clone();
        params.insert(
            if operation.query_param.is_empty() {
                "q".to_string()
            } else {
                operation.query_param.clone()
            },
            serde_json::Value::String(query.to_string()),
        );

        match &operation.pagination {
            PaginationConfig::None => {
                if let Some(limit_param) = &operation.limit_param {
                    params.insert(limit_param.clone(), serde_json::json!(limit));
                }
            }
            PaginationConfig::OffsetLimit {
                page_param,
                size_param,
                start,
                step,
                ..
            } => {
                params.insert(
                    page_param.clone(),
                    serde_json::json!(start + (page_index * step)),
                );
                params.insert(size_param.clone(), serde_json::json!(limit));
            }
            PaginationConfig::PageNumber {
                page_param,
                size_param,
                start,
                ..
            } => {
                params.insert(page_param.clone(), serde_json::json!(start + page_index));
                params.insert(size_param.clone(), serde_json::json!(limit));
            }
        }

        let response = self.execute_request(operation, &params).await?;
        let root = if let Some(path) = &operation.result_path {
            Self::value_at_path(&response, path)
                .cloned()
                .unwrap_or_default()
        } else {
            response
        };
        Ok(root.as_array().cloned().unwrap_or_default())
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for HttpSkillAdapter {
    fn name(&self) -> &str {
        &self.config.id
    }

    fn source_type(&self) -> DataSource {
        self.config.source.clone()
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if !self.config.capabilities.search {
            return Ok(vec![]);
        }

        let operation = &self.config.operations.search;
        let max_pages = match &operation.pagination {
            PaginationConfig::None => 1,
            PaginationConfig::OffsetLimit { max_pages, .. } => *max_pages,
            PaginationConfig::PageNumber { max_pages, .. } => *max_pages,
        }
        .max(1);

        let page_size = limit.max(1);
        let mut items = Vec::new();
        for page_index in 0..max_pages {
            let mut page = self
                .fetch_page(operation, query, page_size, page_index)
                .await?;
            let page_len = page.len();
            items.append(&mut page);
            if items.len() >= limit || page_len == 0 {
                break;
            }
        }

        Ok(items
            .into_iter()
            .take(limit)
            .enumerate()
            .map(|(i, item)| {
                let title = item
                    .get(&self.config.item_mapping.title)
                    .and_then(|v| v.as_str())
                    .unwrap_or(&self.config.name)
                    .to_string();
                let description = item
                    .get(&self.config.item_mapping.description)
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .or_else(|| Some(self.config.description.clone()));
                let id_value = item
                    .get(&self.config.item_mapping.id)
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| format!("{}-{i}", self.config.id));
                let size_bytes = self
                    .config
                    .item_mapping
                    .size_bytes
                    .as_ref()
                    .and_then(|field| item.get(field))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                SearchResult {
                    cid: DatasetCid(format!("skill:{}:{}", self.config.id, id_value)),
                    title: title.clone(),
                    description,
                    tags: self.config.tags.clone(),
                    schema: DatasetSchema {
                        columns: vec![],
                        row_count: 0,
                        size_bytes,
                    },
                    quality: None,
                    price: Price::free(),
                    license: License {
                        spdx_id: "unknown".into(),
                        commercial_use: false,
                        derivative_allowed: false,
                    },
                    provider: Did(format!("skill:{}", self.config.id)),
                    source: self.config.source.clone(),
                    market: None,
                    data_type: infer_data_type_from_title(&title),
                    created_at: chrono::Utc::now(),
                    seller_endpoint: Some(self.config.base_url.clone()),
                    source_attributes: Some(serde_json::json!({
                        "skill_id": self.config.id,
                        "provider_kind": "http_search",
                    })),
                    provider_meta: Some(ProviderMeta {
                        provider_id: self.config.id.clone(),
                        source_family: self.config.source_family,
                        labels: self.config.tags.clone(),
                    }),
                    governance: Some(GovernanceMeta {
                        trust_tier: self.config.governance.trust_tier,
                        rate_limit_hint: self.config.governance.rate_limit_hint.clone(),
                        provenance_hint: self.config.governance.provenance_hint.clone(),
                        compliance_hint: self.config.governance.compliance_hint.clone(),
                    }),
                }
            })
            .collect())
    }
}

fn infer_source_family(source: &str) -> SourceFamily {
    match source.to_ascii_lowercase().as_str() {
        "kaggle" | "huggingface" | "guixu_hub" | "guixu-hub" => SourceFamily::Marketplace,
        "arxiv" | "dblp" | "semantic_scholar" | "datacite_commons" => SourceFamily::Academic,
        "ipfs" | "bittorrent" => SourceFamily::Decentralized,
        "postgresql" | "duckdb" | "spark" | "flink" | "presto" => SourceFamily::DbCatalog,
        "local_file" | "localfile" => SourceFamily::Local,
        "google_dataset_search" | "pan_search" | "open_data_skill" => SourceFamily::WebRegistry,
        _ => SourceFamily::Custom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_v2_http_skill() {
        let raw = r#"
        {
          "spec_version": "2.0",
          "id": "example_registry",
          "name": "Example Registry",
          "description": "Example provider",
          "source": "open_data_skill",
          "tags": ["example"],
          "enabled": true,
          "capabilities": {
            "search": true,
            "lookup": false,
            "download": false,
            "schema_probe": false,
            "sample_preview": false,
            "license_lookup": false
          },
          "governance": {
            "trust_tier": "community"
          },
          "provider": {
            "kind": "http_search",
            "base_url": "https://example.com",
            "operations": {
              "search": {
                "path": "/api/search",
                "method": "POST",
                "query_param": "query",
                "headers": { "Accept": "application/json" },
                "auth": { "kind": "none" },
                "pagination": {
                  "kind": "page_number",
                  "page_param": "page",
                  "size_param": "page_size",
                  "start": 1,
                  "max_pages": 2
                },
                "result_path": "results.items"
              }
            },
            "item_mapping": {
              "title": "title",
              "description": "description",
              "id": "id"
            }
          }
        }
        "#;

        let skill: OpenDataSkillSpec = serde_json::from_str(raw).unwrap();
        validate_skill_spec(&skill).unwrap();
        assert_eq!(skill.spec_version, "2.0");
        assert!(skill.capabilities.search);
        match skill.provider {
            SkillProvider::HttpSearch { operations, .. } => {
                assert_eq!(operations.search.method, "POST");
            }
            _ => panic!("expected http_search provider"),
        }
    }

    #[test]
    fn reject_unsupported_major_version() {
        let skill = OpenDataSkillSpec {
            spec_version: "9.0".into(),
            id: "bad-skill".into(),
            name: "Bad Skill".into(),
            description: "bad".into(),
            source: "open_data_skill".into(),
            tags: vec![],
            enabled: true,
            capabilities: SkillCapabilities::default(),
            governance: SkillGovernance::default(),
            provider: SkillProvider::NativeAdapter {
                adapter: "kaggle".into(),
            },
        };

        assert!(validate_skill_spec(&skill).is_err());
    }
}

#[allow(dead_code)]
fn _is_json_file(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("json")
}
