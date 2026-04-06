// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use data_core::config::{DuckDbCatalog, PostgreSqlCatalog, SqlEndpointCatalog};
use data_core::types::*;
use serde::{Deserialize, Serialize};

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
    pub routing_hints: Vec<String>,
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

impl SkillCapabilities {
    fn enabled_capabilities(&self) -> Vec<SkillCapability> {
        let mut capabilities = Vec::new();
        if self.search {
            capabilities.push(SkillCapability::Search);
        }
        if self.lookup {
            capabilities.push(SkillCapability::Lookup);
        }
        if self.download {
            capabilities.push(SkillCapability::Download);
        }
        if self.schema_probe {
            capabilities.push(SkillCapability::SchemaProbe);
        }
        if self.sample_preview {
            capabilities.push(SkillCapability::SamplePreview);
        }
        if self.license_lookup {
            capabilities.push(SkillCapability::LicenseLookup);
        }
        capabilities
    }

    fn supports(&self, capability: SkillCapability) -> bool {
        self.enabled_capabilities().contains(&capability)
    }
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
    BasicEnv {
        username_env: String,
        password_env: String,
    },
    HeaderEnv {
        header: String,
        env: String,
    },
    OAuthClientCredentials {
        token_url: String,
        client_id_env: String,
        client_secret_env: String,
        #[serde(default)]
        audience: Option<String>,
        #[serde(default)]
        scope: Option<String>,
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
    Cursor {
        cursor_param: String,
        size_param: String,
        #[serde(default)]
        initial_cursor: Option<String>,
        next_cursor_path: String,
        max_pages: usize,
    },
    NextUrl {
        next_url_path: String,
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
    #[serde(default)]
    pub tags: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub download_count: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillFailureKind {
    InvalidSpec,
    AuthMissing,
    AuthFailed,
    RateLimited,
    UpstreamUnavailable,
    BadResponse,
    MappingFailed,
    UnsupportedOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExecutionMetric {
    pub skill_id: String,
    pub provider_kind: String,
    pub operation: String,
    pub pages_fetched: usize,
    pub result_count: usize,
    pub auth_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSkillProfile {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub source_family: SourceFamily,
    pub capabilities: Vec<SkillCapability>,
    pub labels: Vec<String>,
    pub routing_hints: Vec<String>,
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

pub fn load_data_skill_profiles() -> Result<Vec<DataSkillProfile>> {
    Ok(load_open_data_skills()?
        .into_iter()
        .map(|skill| DataSkillProfile {
            skill_id: skill.id.clone(),
            name: skill.name.clone(),
            description: skill.description.clone(),
            source_family: infer_source_family(&skill.source),
            capabilities: skill.capabilities.enabled_capabilities(),
            labels: skill.tags.clone(),
            routing_hints: if skill.routing_hints.is_empty() {
                skill.tags.clone()
            } else {
                skill.routing_hints.clone()
            },
        })
        .collect())
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

pub fn validate_skill_spec(skill: &OpenDataSkillSpec) -> Result<()> {
    if !(skill.spec_version.starts_with("1.") || skill.spec_version.starts_with("2.")) {
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
        "defillama" => Box::new(DefiLlamaAdapter::default()),
        "rwa_xyz" => Box::new(RwaXyzAdapter::default()),
        "pan_search" => Box::new(PanSearchAdapter::default()),
        "dblp" => Box::new(DblpAdapter::default()),
        "semantic_scholar" => Box::new(SemanticScholarAdapter::default()),
        "arxiv" => Box::new(ArxivAdapter::default()),
        other => return Err(anyhow!("unknown native adapter: {other}")),
    })
}

pub fn parse_skill_source(source: &str) -> Result<DataSource> {
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

#[derive(Debug, Clone, Copy)]
enum ExecutedOperation {
    Search,
    Lookup,
    Download,
    SchemaProbe,
}

pub async fn execute_skill_operation(
    skill: &OpenDataSkillSpec,
    operation_name: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>> {
    let SkillProvider::HttpSearch {
        base_url,
        operations,
        item_mapping,
    } = &skill.provider
    else {
        return Err(anyhow!("skill is not http_search: {}", skill.id));
    };

    let adapter = HttpSkillAdapter::new(HttpSkillAdapterConfig {
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
    });

    let operation = match operation_name {
        "search" => ExecutedOperation::Search,
        "lookup" => ExecutedOperation::Lookup,
        "download" => ExecutedOperation::Download,
        "schema_probe" => ExecutedOperation::SchemaProbe,
        other => return Err(anyhow!("unsupported skill operation: {other}")),
    };

    adapter
        .execute_named_operation(operation, query, limit)
        .await
}

impl HttpSkillAdapter {
    fn classify_failure(error: &anyhow::Error) -> SkillFailureKind {
        let message = error.to_string().to_ascii_lowercase();
        if message.contains("missing") && message.contains("env") {
            SkillFailureKind::AuthMissing
        } else if message.contains("401") || message.contains("403") || message.contains("oauth") {
            SkillFailureKind::AuthFailed
        } else if message.contains("429") || message.contains("rate") {
            SkillFailureKind::RateLimited
        } else if message.contains("timeout")
            || message.contains("unavailable")
            || message.contains("connect")
        {
            SkillFailureKind::UpstreamUnavailable
        } else if message.contains("unsupported") {
            SkillFailureKind::UnsupportedOperation
        } else {
            SkillFailureKind::BadResponse
        }
    }

    fn auth_kind_name(auth: &SkillAuth) -> &'static str {
        match auth {
            SkillAuth::None => "none",
            SkillAuth::BearerEnv { .. } => "bearer_env",
            SkillAuth::BasicEnv { .. } => "basic_env",
            SkillAuth::HeaderEnv { .. } => "header_env",
            SkillAuth::OAuthClientCredentials { .. } => "oauth_client_credentials",
        }
    }

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
            if let Some((field, index_part)) = part.split_once('[') {
                let index = index_part.strip_suffix(']')?.parse::<usize>().ok()?;
                current = current.get(field)?;
                current = current.get(index)?;
            } else {
                current = current.get(part)?;
            }
        }
        Some(current)
    }

    fn string_at_path(value: &serde_json::Value, path: &str) -> Option<String> {
        Self::value_at_path(value, path).and_then(|v| v.as_str().map(ToString::to_string))
    }

    fn u64_at_path(value: &serde_json::Value, path: &str) -> Option<u64> {
        Self::value_at_path(value, path).and_then(|v| v.as_u64())
    }

    async fn base_headers(&self, operation: &HttpOperation) -> Result<reqwest::header::HeaderMap> {
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
                let token =
                    std::env::var(env).map_err(|_| anyhow!("missing bearer token env: {env}"))?;
                let value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))?;
                headers.insert(reqwest::header::AUTHORIZATION, value);
            }
            SkillAuth::BasicEnv {
                username_env,
                password_env,
            } => {
                use base64::Engine as _;
                let username = std::env::var(username_env)
                    .map_err(|_| anyhow!("missing basic auth username env: {username_env}"))?;
                let password = std::env::var(password_env)
                    .map_err(|_| anyhow!("missing basic auth password env: {password_env}"))?;
                let encoded = base64::engine::general_purpose::STANDARD
                    .encode(format!("{username}:{password}"));
                let value = reqwest::header::HeaderValue::from_str(&format!("Basic {encoded}"))?;
                headers.insert(reqwest::header::AUTHORIZATION, value);
            }
            SkillAuth::HeaderEnv { header, env } => {
                let token =
                    std::env::var(env).map_err(|_| anyhow!("missing header auth env: {env}"))?;
                let header_name = reqwest::header::HeaderName::from_bytes(header.as_bytes())?;
                let header_value = reqwest::header::HeaderValue::from_str(&token)?;
                headers.insert(header_name, header_value);
            }
            SkillAuth::OAuthClientCredentials {
                token_url,
                client_id_env,
                client_secret_env,
                audience,
                scope,
            } => {
                let client_id = std::env::var(client_id_env)
                    .map_err(|_| anyhow!("missing oauth client id env: {client_id_env}"))?;
                let client_secret = std::env::var(client_secret_env)
                    .map_err(|_| anyhow!("missing oauth client secret env: {client_secret_env}"))?;
                let mut form = vec![
                    ("grant_type", "client_credentials".to_string()),
                    ("client_id", client_id),
                    ("client_secret", client_secret),
                ];
                if let Some(audience) = audience {
                    form.push(("audience", audience.clone()));
                }
                if let Some(scope) = scope {
                    form.push(("scope", scope.clone()));
                }
                let token_response = self
                    .client
                    .post(token_url)
                    .form(&form)
                    .send()
                    .await?
                    .error_for_status()?;
                let token_json = token_response.json::<serde_json::Value>().await?;
                let access_token = token_json
                    .get("access_token")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("oauth token response missing access_token"))?;
                let value =
                    reqwest::header::HeaderValue::from_str(&format!("Bearer {access_token}"))?;
                headers.insert(reqwest::header::AUTHORIZATION, value);
            }
        }

        Ok(headers)
    }

    async fn execute_request(
        &self,
        operation: &HttpOperation,
        params: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let headers = self.base_headers(operation).await?;
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

    async fn execute_absolute_get(
        &self,
        url: &str,
        operation: &HttpOperation,
    ) -> Result<serde_json::Value> {
        let headers = self.base_headers(operation).await?;
        let response = self
            .client
            .get(url)
            .headers(headers)
            .send()
            .await?
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
            PaginationConfig::Cursor {
                cursor_param,
                size_param,
                initial_cursor,
                ..
            } => {
                if page_index == 0 {
                    if let Some(initial_cursor) = initial_cursor {
                        params.insert(cursor_param.clone(), serde_json::json!(initial_cursor));
                    }
                }
                params.insert(size_param.clone(), serde_json::json!(limit));
            }
            PaginationConfig::NextUrl { .. } => {}
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

    async fn execute_named_operation(
        &self,
        operation_name: ExecutedOperation,
        query: &str,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>> {
        let operation = match operation_name {
            ExecutedOperation::Search => &self.config.operations.search,
            ExecutedOperation::Lookup => self
                .config
                .operations
                .lookup
                .as_ref()
                .ok_or_else(|| anyhow!("lookup operation not configured"))?,
            ExecutedOperation::Download => self
                .config
                .operations
                .download
                .as_ref()
                .ok_or_else(|| anyhow!("download operation not configured"))?,
            ExecutedOperation::SchemaProbe => self
                .config
                .operations
                .schema_probe
                .as_ref()
                .ok_or_else(|| anyhow!("schema_probe operation not configured"))?,
        };

        let max_pages = match &operation.pagination {
            PaginationConfig::None => 1,
            PaginationConfig::OffsetLimit { max_pages, .. } => *max_pages,
            PaginationConfig::PageNumber { max_pages, .. } => *max_pages,
            PaginationConfig::Cursor { max_pages, .. } => *max_pages,
            PaginationConfig::NextUrl { max_pages, .. } => *max_pages,
        }
        .max(1);

        let mut items = Vec::new();
        let mut next_url: Option<String> = None;
        for page_index in 0..max_pages {
            let page_result: Result<(Vec<serde_json::Value>, Option<String>)> =
                if matches!(&operation.pagination, PaginationConfig::NextUrl { .. }) {
                    let response = if page_index == 0 {
                        let mut params = operation.static_params.clone();
                        params.insert(
                            if operation.query_param.is_empty() {
                                "q".to_string()
                            } else {
                                operation.query_param.clone()
                            },
                            serde_json::Value::String(query.to_string()),
                        );
                        if let Some(limit_param) = &operation.limit_param {
                            params.insert(limit_param.clone(), serde_json::json!(limit));
                        }
                        self.execute_request(operation, &params).await
                    } else {
                        let url = next_url.clone().ok_or_else(|| {
                            anyhow!("next_url pagination missing continuation url")
                        })?;
                        self.execute_absolute_get(&url, operation).await
                    };

                    response.map(|response| {
                        let next = match &operation.pagination {
                            PaginationConfig::NextUrl { next_url_path, .. } => {
                                Self::string_at_path(&response, next_url_path)
                            }
                            _ => None,
                        };
                        let root = if let Some(path) = &operation.result_path {
                            Self::value_at_path(&response, path)
                                .cloned()
                                .unwrap_or_default()
                        } else {
                            response
                        };
                        (root.as_array().cloned().unwrap_or_default(), next)
                    })
                } else {
                    self.fetch_page(operation, query, limit.max(1), page_index)
                        .await
                        .map(|page| (page, None))
                };

            let (mut page, extracted_next_url) = match page_result {
                Ok(page) => page,
                Err(error) => {
                    let failure_kind = Self::classify_failure(&error);
                    tracing::warn!(
                        skill_id = %self.config.id,
                        operation = ?operation_name,
                        failure_kind = ?failure_kind,
                        error = %error,
                        "open data skill operation failed"
                    );
                    return Err(error);
                }
            };
            next_url = extracted_next_url;
            let page_len = page.len();
            items.append(&mut page);
            if items.len() >= limit || page_len == 0 {
                break;
            }
        }

        let metric = SkillExecutionMetric {
            skill_id: self.config.id.clone(),
            provider_kind: "http_search".into(),
            operation: format!("{:?}", operation_name).to_lowercase(),
            pages_fetched: max_pages,
            result_count: items.len(),
            auth_kind: Self::auth_kind_name(&operation.auth).into(),
        };
        tracing::info!(skill_id = %metric.skill_id, operation = %metric.operation, result_count = metric.result_count, pages_fetched = metric.pages_fetched, auth_kind = %metric.auth_kind, "open data skill operation executed");

        Ok(items)
    }

    fn normalize_search_item(&self, item: serde_json::Value, i: usize) -> SearchResult {
        let title = item
            .get(&self.config.item_mapping.title)
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
            .or_else(|| Self::string_at_path(&item, &self.config.item_mapping.title))
            .unwrap_or_else(|| self.config.name.clone());
        let description = item
            .get(&self.config.item_mapping.description)
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| Self::string_at_path(&item, &self.config.item_mapping.description))
            .or_else(|| Some(self.config.description.clone()));
        let id_value = item
            .get(&self.config.item_mapping.id)
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| Self::string_at_path(&item, &self.config.item_mapping.id))
            .unwrap_or_else(|| format!("{}-{i}", self.config.id));
        let size_bytes = self
            .config
            .item_mapping
            .size_bytes
            .as_ref()
            .and_then(|field| {
                item.get(field)
                    .and_then(|v| v.as_u64())
                    .or_else(|| Self::u64_at_path(&item, field))
            })
            .unwrap_or(0);
        let tags = self
            .config
            .item_mapping
            .tags
            .as_ref()
            .and_then(|field| item.get(field))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| self.config.tags.clone());
        let license_id = self
            .config
            .item_mapping
            .license
            .as_ref()
            .and_then(|field| {
                item.get(field)
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string)
                    .or_else(|| Self::string_at_path(&item, field))
            })
            .unwrap_or_else(|| "unknown".to_string());
        let created_at = self
            .config
            .item_mapping
            .created_at
            .as_ref()
            .and_then(|field| {
                item.get(field)
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string)
                    .or_else(|| Self::string_at_path(&item, field))
            })
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);
        let download_count = self
            .config
            .item_mapping
            .download_count
            .as_ref()
            .and_then(|field| {
                item.get(field)
                    .and_then(|v| v.as_u64())
                    .or_else(|| Self::u64_at_path(&item, field))
            })
            .unwrap_or(0);

        SearchResult {
            cid: DatasetCid(format!("skill:{}:{}", self.config.id, id_value)),
            title: title.clone(),
            description,
            tags,
            schema: DatasetSchema {
                columns: vec![],
                row_count: 0,
                size_bytes,
            },
            quality: None,
            price: Price::free(),
            license: License {
                spdx_id: license_id,
                commercial_use: false,
                derivative_allowed: false,
            },
            provider: Did(format!("skill:{}", self.config.id)),
            source: self.config.source.clone(),
            market: Some(DatasetMarketStats {
                download_count,
                review_count: 0,
                trade_count: 0,
            }),
            data_type: infer_data_type_from_title(&title),
            created_at,
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
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for HttpSkillAdapter {
    fn name(&self) -> &str {
        &self.config.id
    }

    fn skill_id(&self) -> &str {
        &self.config.id
    }

    fn source_family(&self) -> SourceFamily {
        self.config.source_family
    }

    fn capabilities(&self) -> Vec<SkillCapability> {
        self.config.capabilities.enabled_capabilities()
    }

    fn labels(&self) -> Vec<String> {
        self.config.tags.clone()
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if !self.config.capabilities.supports(SkillCapability::Search) {
            return Ok(vec![]);
        }

        Ok(self
            .execute_named_operation(ExecutedOperation::Search, query, limit)
            .await?
            .into_iter()
            .take(limit)
            .enumerate()
            .map(|(i, item)| self.normalize_search_item(item, i))
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
        "google_dataset_search"
        | "pan_search"
        | "open_data_skill"
        | "defillama"
        | "rwa_xyz"
        | "rwaxyz"
        | "thegraph" => SourceFamily::WebRegistry,
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

    #[test]
    fn classify_failure_auth_missing() {
        let error = anyhow!("missing bearer token env: HF_TOKEN");
        assert_eq!(
            HttpSkillAdapter::classify_failure(&error),
            SkillFailureKind::AuthMissing
        );
    }
}

#[allow(dead_code)]
fn _is_json_file(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("json")
}
