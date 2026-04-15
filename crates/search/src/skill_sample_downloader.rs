// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Generic sample downloader that works with Open Data Skill configurations.
//!
//! This module provides a `SkillSampleDownloader` that can download and parse
//! sample data from any data source that declares a `sample` configuration in
//! its skill JSON. This approach avoids per-source code changes — adding support
//! for a new source only requires updating the skill JSON.
//!
//! # Skill JSON Configuration
//!
//! Skills that support `sample_preview` should include a `sample` section:
//!
//! ```json
//! {
//!   "sample": {
//!     "parse_mode": "archive",
//!     "download_endpoint": "/api/download/{id}",
//!     "id_param": "id",
//!     "range_header": true,
//!     "default_max_bytes": 65536,
//!     "auth": { "kind": "bearer_env", "env": "HF_TOKEN" }
//!   }
//! }
//! ```
//!
//! # Parse Modes
//!
//! - `archive`: ZIP file containing multiple files (csv, json, jsonl, txt, images)
//! - `blob`: Single file (auto-detect type from extension)
//! - `csv`: CSV file with header row
//! - `json`: JSON array file
//! - `jsonl`: JSON Lines file (one JSON object per line)

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use data_core::metadata::DatasetMetadata;
use data_core::types::{DataSource, SearchResult};
use reqwest::Client;

use crate::adapters::{SkillAuth, SkillSampleParseMode, SkillSampleProvider};
use crate::engine::SamplePlan;
use crate::sample_eval::{DownloadedSample, SampleDownloadOutcome, SampleDownloader};

/// Flattened sample download configuration used at runtime.
///
/// Built from a skill's [`SkillSampleProvider`] declaration plus the
/// provider-level `base_url`.
#[derive(Debug, Clone)]
pub struct SkillSampleConfig {
    pub download_endpoint: String,
    pub id_param: String,
    pub range_header: bool,
    pub default_max_bytes: usize,
    pub auth: SkillAuth,
    pub parse_mode: SkillSampleParseMode,
    pub prefer_extensions: Vec<String>,
}

impl SkillSampleConfig {
    /// Build a runtime config from a skill's sample provider declaration.
    ///
    /// `provider_base_url` is the `base_url` from the enclosing
    /// `SkillProvider::HttpSearch` (if any). When `SkillSampleProvider.base_url`
    /// is set, it takes precedence.
    pub fn from_provider(sample: &SkillSampleProvider, provider_base_url: Option<&str>) -> Self {
        let base = sample
            .base_url
            .as_deref()
            .or(provider_base_url)
            .unwrap_or_default();
        let download_endpoint =
            if sample.endpoint.starts_with("http://") || sample.endpoint.starts_with("https://") {
                sample.endpoint.clone()
            } else {
                format!("{}{}", base.trim_end_matches('/'), sample.endpoint)
            };
        Self {
            download_endpoint,
            id_param: sample.id_param.clone(),
            range_header: sample.range_header,
            default_max_bytes: sample.max_bytes,
            auth: sample.auth.clone(),
            parse_mode: sample.parse_mode,
            prefer_extensions: sample.prefer_extensions.clone(),
        }
    }
}

/// Per-skill sample configuration with resolved auth token.
#[derive(Debug, Clone)]
struct ResolvedSkillSample {
    config: SkillSampleConfig,
    token: Option<String>,
}

/// Sample downloader that uses Open Data Skill configurations.
///
/// This downloader inspects each skill's `sample` configuration and uses
/// HTTP GET requests to fetch sample data, then parses according to `parse_mode`.
pub struct SkillSampleDownloader {
    client: Client,
    skills: Vec<ResolvedSkillSample>,
    extract_root: PathBuf,
}

impl SkillSampleDownloader {
    /// Create a new downloader from a list of skill configurations.
    pub fn new(skills: Vec<SkillSampleConfig>) -> Self {
        let skills = skills
            .into_iter()
            .map(|config| ResolvedSkillSample {
                token: resolve_auth_token(&config.auth),
                config,
            })
            .collect();

        Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(30))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| Client::new()),
            skills,
            extract_root: std::env::temp_dir().join("guixu-skill-sample-cache"),
        }
    }

    /// Add a skill configuration.
    pub fn with_skill(mut self, config: SkillSampleConfig) -> Self {
        self.skills.push(ResolvedSkillSample {
            token: resolve_auth_token(&config.auth),
            config,
        });
        self
    }

    /// Set the extract root directory.
    pub fn with_extract_root(mut self, path: PathBuf) -> Self {
        self.extract_root = path;
        self
    }

    /// Find the skill configuration that matches the search result's source.
    fn find_matching_skill(&self, result: &SearchResult) -> Option<&ResolvedSkillSample> {
        self.skills.iter().find(|_skill| {
            // Match by source or by source_attributes.skill_id
            if let Some(attrs) = &result.source_attributes {
                if let Some(_skill_id) = attrs.get("skill_id").and_then(|v| v.as_str()) {
                    // This is for HTTP-based skills
                    return true;
                }
            }
            // Match by DataSource enum
            matches!(result.source, DataSource::HuggingFace | DataSource::Kaggle)
        })
    }

    /// Build the download URL for a skill.
    fn build_download_url(&self, skill: &ResolvedSkillSample, result: &SearchResult) -> String {
        let id = extract_dataset_id(result);
        let endpoint = &skill.config.download_endpoint;

        if endpoint.contains("{id}") {
            endpoint.replace("{id}", &id)
        } else {
            format!("{}?{}={}", endpoint, skill.config.id_param, id)
        }
    }

    /// Build request with authentication headers.
    fn build_request(&self, skill: &ResolvedSkillSample, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.get(url);

        // Add range header if configured
        if skill.config.range_header {
            let max_bytes = skill.config.default_max_bytes;
            req = req.header("Range", format!("bytes=0-{}", max_bytes));
        }

        // Add auth
        match &skill.config.auth {
            SkillAuth::None => {}
            SkillAuth::BearerEnv { .. } => {
                if let Some(token) = &skill.token {
                    req = req.header("Authorization", format!("Bearer {}", token));
                }
            }
            SkillAuth::BasicEnv { .. } => {
                if let Some(token) = &skill.token {
                    req = req.header("Authorization", format!("Basic {}", token));
                }
            }
            SkillAuth::HeaderEnv { header, .. } => {
                if let Some(token) = &skill.token {
                    req = req.header(header, token);
                }
            }
            SkillAuth::OAuthClientCredentials { .. } => {
                // OAuth not supported for sample downloads; skip.
            }
        }

        req
    }

    /// Download and parse a sample.
    async fn download_and_parse(
        &self,
        skill: &ResolvedSkillSample,
        result: &SearchResult,
        plan: &SamplePlan,
    ) -> Result<DownloadedSample> {
        let url = self.build_download_url(skill, result);
        let id = extract_dataset_id(result);

        let response = self
            .build_request(skill, &url)
            .send()
            .await
            .with_context(|| format!("sample download request failed for {id}"))?
            .error_for_status()
            .with_context(|| format!("sample download returned error for {id}"))?;

        let final_url = response.url().to_string();
        let bytes = response
            .bytes()
            .await
            .with_context(|| format!("read sample bytes for {id}"))?;

        if bytes.is_empty() {
            return Err(anyhow!("sample download returned empty content for {id}"));
        }

        let max_records = plan.max_rows.max(1) as usize;
        let max_bytes = plan.max_bytes.max(1) as usize;

        match skill.config.parse_mode {
            SkillSampleParseMode::Archive => parse_sample_archive(
                bytes.as_ref(),
                &id,
                &final_url,
                &self.extract_root,
                max_records,
                max_bytes,
                &skill.config.prefer_extensions,
            ),
            SkillSampleParseMode::Blob => parse_sample_blob(
                bytes.as_ref(),
                &id,
                &final_url,
                &self.extract_root,
                max_bytes,
            ),
            SkillSampleParseMode::Csv => {
                parse_csv_sample(bytes.as_ref(), &id, &final_url, max_records, max_bytes)
            }
            SkillSampleParseMode::Json => {
                parse_json_sample(bytes.as_ref(), &id, &final_url, max_records, max_bytes)
            }
            SkillSampleParseMode::Jsonl => {
                parse_jsonl_sample(bytes.as_ref(), &id, &final_url, max_records, max_bytes)
            }
        }
    }
}

fn resolve_auth_token(auth: &SkillAuth) -> Option<String> {
    match auth {
        SkillAuth::None => None,
        SkillAuth::BearerEnv { env } => std::env::var(env).ok(),
        SkillAuth::BasicEnv {
            username_env,
            password_env,
        } => {
            let username = std::env::var(username_env).ok()?;
            let password = std::env::var(password_env).ok()?;
            use base64::Engine as _;
            Some(base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}")))
        }
        SkillAuth::HeaderEnv { env, .. } => std::env::var(env).ok(),
        SkillAuth::OAuthClientCredentials { .. } => None,
    }
}

fn extract_dataset_id(result: &SearchResult) -> String {
    // Try to extract ID from CID
    if let Some(cid_str) = result.cid.0.strip_prefix("hf:") {
        return cid_str.to_string();
    }
    if let Some(cid_str) = result.cid.0.strip_prefix("kaggle:") {
        return cid_str.to_string();
    }
    if let Some(cid_str) = result.cid.0.strip_prefix("skill:") {
        // skill:source:id format
        if let Some(colon_pos) = cid_str.find(':') {
            return cid_str[colon_pos + 1..].to_string();
        }
        return cid_str.to_string();
    }
    result.cid.0.clone()
}

// ============================================================================
// Parsing functions for different content types
// ============================================================================

/// Parse a ZIP archive containing sample files.
pub fn parse_sample_archive(
    archive_bytes: &[u8],
    id: &str,
    download_url: &str,
    extract_root: &Path,
    max_records: usize,
    max_text_chars: usize,
    prefer_extensions: &[String],
) -> Result<DownloadedSample> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(archive_bytes)).context("open sample zip archive")?;

    let mut image_entries_by_stem: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut image_entry_names: Vec<String> = Vec::new();
    let mut text_entry_names: Vec<String> = Vec::new();

    // Collect entries by type
    for i in 0..archive.len() {
        let file = archive.by_index(i).context("read archive entry")?;
        if !file.is_file() {
            continue;
        }
        let name = file.name().to_string();
        let Some(file_name) = Path::new(&name).file_name().and_then(|v| v.to_str()) else {
            continue;
        };

        let lower_name = file_name.to_ascii_lowercase();
        if is_image_path(file_name) {
            image_entries_by_stem.insert(stem_key(&lower_name), name.clone());
            image_entry_names.push(name);
        } else if is_text_sample_path(file_name) {
            text_entry_names.push(name);
        }
    }

    // Prioritize preferred extensions
    if !prefer_extensions.is_empty() {
        text_entry_names.sort_by(|a, b| {
            let a_ext = Path::new(a)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let b_ext = Path::new(b)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let a优先级 = prefer_extensions
                .iter()
                .position(|e| e.eq_ignore_ascii_case(a_ext));
            let b优先级 = prefer_extensions
                .iter()
                .position(|e| e.eq_ignore_ascii_case(b_ext));
            match (a优先级, b优先级) {
                (Some(_), Some(_)) => std::cmp::Ordering::Equal,
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.cmp(b),
            }
        });
    }

    let mut records = Vec::new();
    let mut extracted_images: std::collections::HashMap<String, PathBuf> =
        std::collections::HashMap::new();

    for entry_name in text_entry_names.into_iter().take(max_records) {
        let bytes = {
            let mut file = archive
                .by_name(&entry_name)
                .context("read archive text entry")?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .context("read archive text bytes")?;
            bytes
        };

        let content = summarize_sample_entry(&entry_name, &bytes, max_text_chars);
        if content.trim().is_empty() {
            continue;
        }

        let mut metadata = serde_json::json!({
            "id": id,
            "archive_path": entry_name,
            "kind": archive_entry_kind(&entry_name),
            "source_download_url": download_url,
        });

        let entry_stem = stem_key(
            Path::new(&entry_name)
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default(),
        );

        if let Some(image_entry_name) = image_entries_by_stem.get(&entry_stem) {
            let image_path = extract_archive_entry_image(
                &mut archive,
                image_entry_name,
                extract_root,
                &mut extracted_images,
            )?;
            metadata["local_image_path"] =
                serde_json::Value::String(image_path.display().to_string());
            metadata["local_image_mime_type"] =
                serde_json::Value::String(guess_image_mime_type(&image_path).to_string());
        }

        records.push(crate::sample_eval::SampleRecord {
            id: entry_name.clone(),
            content,
            metadata,
        });
    }

    // Fall back to images if no text records
    if records.is_empty() {
        for image_entry_name in image_entry_names.into_iter().take(max_records) {
            let image_path = extract_archive_entry_image(
                &mut archive,
                &image_entry_name,
                extract_root,
                &mut extracted_images,
            )?;
            records.push(crate::sample_eval::SampleRecord {
                id: image_entry_name.clone(),
                content: format!(
                    "image sample from dataset {}",
                    Path::new(&image_entry_name)
                        .file_name()
                        .and_then(|v| v.to_str())
                        .unwrap_or(&image_entry_name)
                ),
                metadata: serde_json::json!({
                    "id": id,
                    "archive_path": image_entry_name,
                    "kind": "image",
                    "source_download_url": download_url,
                    "local_image_path": image_path.display().to_string(),
                    "local_image_mime_type": guess_image_mime_type(&image_path),
                }),
            });
        }
    }

    Ok(DownloadedSample {
        sampled_rows: records.len() as u64,
        sampled_bytes: archive_bytes.len() as u64,
        summary: Some(format!(
            "downloaded sample archive for {} with {} records",
            id,
            records.len()
        )),
        records,
    })
}

/// Parse a single file (auto-detect type).
pub fn parse_sample_blob(
    blob_bytes: &[u8],
    id: &str,
    download_url: &str,
    extract_root: &Path,
    max_text_chars: usize,
) -> Result<DownloadedSample> {
    let guessed_name = download_url
        .split('?')
        .next()
        .and_then(|v| v.rsplit('/').next())
        .filter(|v| !v.is_empty())
        .unwrap_or("sample.bin");

    let sample_record = if is_image_path(guessed_name) {
        std::fs::create_dir_all(extract_root).context("create sample extract root")?;
        let image_path = extract_root.join(
            Path::new(guessed_name)
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("sample.bin")),
        );
        std::fs::write(&image_path, blob_bytes).context("write sample image")?;

        crate::sample_eval::SampleRecord {
            id: guessed_name.to_string(),
            content: format!("image sample {} from dataset {}", guessed_name, id),
            metadata: serde_json::json!({
                "id": id,
                "kind": "image",
                "source_download_url": download_url,
                "local_image_path": image_path.display().to_string(),
                "local_image_mime_type": guess_image_mime_type(&image_path),
            }),
        }
    } else {
        let kind = archive_entry_kind(guessed_name);
        crate::sample_eval::SampleRecord {
            id: guessed_name.to_string(),
            content: summarize_sample_entry(guessed_name, blob_bytes, max_text_chars),
            metadata: serde_json::json!({
                "id": id,
                "kind": kind,
                "source_download_url": download_url,
            }),
        }
    };

    Ok(DownloadedSample {
        sampled_rows: 1,
        sampled_bytes: blob_bytes.len() as u64,
        summary: Some(format!("downloaded direct sample blob for {}", id)),
        records: vec![sample_record],
    })
}

/// Parse a CSV file.
pub fn parse_csv_sample(
    csv_bytes: &[u8],
    id: &str,
    download_url: &str,
    max_records: usize,
    _max_text_chars: usize,
) -> Result<DownloadedSample> {
    let text = String::from_utf8_lossy(csv_bytes);
    let mut records = Vec::new();
    let mut lines = text.lines();

    // Get header
    let header = match lines.next() {
        Some(h) => h.to_string(),
        None => {
            return Ok(DownloadedSample {
                sampled_rows: 0,
                sampled_bytes: csv_bytes.len() as u64,
                summary: Some(format!("empty CSV for {}", id)),
                records: vec![],
            })
        }
    };

    for (i, line) in lines.enumerate() {
        if i >= max_records {
            break;
        }
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() {
            continue;
        }

        records.push(crate::sample_eval::SampleRecord {
            id: format!("{}_row_{}", id, i),
            content: format!("{} | {}", header, trimmed_line),
            metadata: serde_json::json!({
                "id": id,
                "kind": "tabular",
                "source_download_url": download_url,
                "row_number": i,
            }),
        });
    }

    Ok(DownloadedSample {
        sampled_rows: records.len() as u64,
        sampled_bytes: csv_bytes.len() as u64,
        summary: Some(format!("parsed {} rows from CSV for {}", records.len(), id)),
        records,
    })
}

/// Parse a JSON array file.
pub fn parse_json_sample(
    json_bytes: &[u8],
    id: &str,
    download_url: &str,
    max_records: usize,
    max_text_chars: usize,
) -> Result<DownloadedSample> {
    let json: serde_json::Value =
        serde_json::from_slice(json_bytes).context("parse JSON sample")?;

    let array = match &json {
        serde_json::Value::Array(arr) => arr.clone(),
        _ => {
            return Ok(DownloadedSample {
                sampled_rows: 0,
                sampled_bytes: json_bytes.len() as u64,
                summary: Some(format!("JSON sample for {} is not an array", id)),
                records: vec![],
            });
        }
    };

    let mut records = Vec::new();
    let json_text = serde_json::to_string_pretty(&json).unwrap_or_default();

    for (i, item) in array.iter().enumerate().take(max_records) {
        let content = if json_text.len() > max_text_chars {
            serde_json::to_string(item).unwrap_or_default()
        } else {
            json_text.clone()
        };

        records.push(crate::sample_eval::SampleRecord {
            id: format!("{}_item_{}", id, i),
            content: truncate_for_prompt(&content, max_text_chars),
            metadata: serde_json::json!({
                "id": id,
                "kind": "json",
                "source_download_url": download_url,
                "index": i,
            }),
        });
    }

    Ok(DownloadedSample {
        sampled_rows: records.len() as u64,
        sampled_bytes: json_bytes.len() as u64,
        summary: Some(format!(
            "parsed {} items from JSON for {}",
            records.len(),
            id
        )),
        records,
    })
}

/// Parse a JSON Lines file.
pub fn parse_jsonl_sample(
    jsonl_bytes: &[u8],
    id: &str,
    download_url: &str,
    max_records: usize,
    max_text_chars: usize,
) -> Result<DownloadedSample> {
    let text = String::from_utf8_lossy(jsonl_bytes);
    let mut records = Vec::new();

    for (i, line) in text.lines().enumerate() {
        if i >= max_records {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let _json: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let content = truncate_for_prompt(trimmed, max_text_chars);

        records.push(crate::sample_eval::SampleRecord {
            id: format!("{}_line_{}", id, i),
            content,
            metadata: serde_json::json!({
                "id": id,
                "kind": "jsonl",
                "source_download_url": download_url,
                "line_number": i,
            }),
        });
    }

    Ok(DownloadedSample {
        sampled_rows: records.len() as u64,
        sampled_bytes: jsonl_bytes.len() as u64,
        summary: Some(format!(
            "parsed {} lines from JSONL for {}",
            records.len(),
            id
        )),
        records,
    })
}

// ============================================================================
// Helper functions (shared with GuixuHubSampleDownloader)
// ============================================================================

fn extract_archive_entry_image(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    entry_name: &str,
    extract_root: &Path,
    extracted_images: &mut std::collections::HashMap<String, PathBuf>,
) -> Result<PathBuf> {
    if let Some(existing) = extracted_images.get(entry_name) {
        return Ok(existing.clone());
    }

    let mut file = archive
        .by_name(entry_name)
        .context("read archive image entry")?;
    let file_name = Path::new(entry_name)
        .file_name()
        .map(|v| v.to_owned())
        .unwrap_or_else(|| std::ffi::OsString::from("sample-image.bin"));

    std::fs::create_dir_all(extract_root).context("create sample extract root")?;
    let output_path = extract_root.join(&file_name);

    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .context("read archive image bytes")?;
    std::fs::write(&output_path, &bytes).context("write extracted image")?;

    extracted_images.insert(entry_name.to_string(), output_path.clone());
    Ok(output_path)
}

fn is_image_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|v| v.to_str())
            .map(|v| v.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "bmp" | "webp" | "gif")
    )
}

fn is_text_sample_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|v| v.to_str())
            .map(|v| v.to_ascii_lowercase())
            .as_deref(),
        Some("xml" | "txt" | "json" | "jsonl" | "csv" | "tsv" | "md")
    )
}

fn stem_key(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn archive_entry_kind(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| v.to_ascii_lowercase())
        .as_deref()
    {
        Some("xml") => "xml",
        Some("json" | "jsonl") => "json",
        Some("csv" | "tsv") => "tabular",
        Some("md" | "txt") => "text",
        Some("jpg" | "jpeg" | "png" | "bmp" | "webp" | "gif") => "image",
        _ => "binary",
    }
}

fn summarize_sample_entry(path: &str, bytes: &[u8], max_text_chars: usize) -> String {
    let extension = Path::new(path)
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| v.to_ascii_lowercase())
        .unwrap_or_default();

    let text = String::from_utf8_lossy(bytes);
    let raw = match extension.as_str() {
        "xml" => {
            let labels = extract_xml_name_labels(&text);
            if labels.is_empty() {
                text.lines().take(8).collect::<Vec<_>>().join(" ")
            } else {
                format!("xml labels: {}", labels.join(" "))
            }
        }
        "json" | "jsonl" | "csv" | "tsv" | "txt" => {
            text.lines().take(8).collect::<Vec<_>>().join(" ")
        }
        "md" => text.lines().take(12).collect::<Vec<_>>().join(" "),
        _ => text.lines().take(8).collect::<Vec<_>>().join(" "),
    };

    truncate_for_prompt(&raw, max_text_chars)
}

fn extract_xml_name_labels(contents: &str) -> Vec<String> {
    let mut labels = Vec::new();
    let mut cursor = contents;
    while let Some(start) = cursor.find("<name>") {
        let after_start = &cursor[start + "<name>".len()..];
        let Some(end) = after_start.find("</name>") else {
            break;
        };
        let label = after_start[..end].trim().to_lowercase();
        if !label.is_empty() && !labels.contains(&label) {
            labels.push(label);
        }
        cursor = &after_start[end + "</name>".len()..];
    }
    labels
}

pub fn guess_image_mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| v.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("gif") => "image/gif",
        _ => "image/jpeg",
    }
}

fn truncate_for_prompt(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect::<String>()
}

#[async_trait::async_trait]
impl SampleDownloader for SkillSampleDownloader {
    async fn download_sample(
        &self,
        result: &SearchResult,
        _metadata: &DatasetMetadata,
        plan: &SamplePlan,
    ) -> Result<SampleDownloadOutcome> {
        let Some(skill) = self.find_matching_skill(result) else {
            return Ok(SampleDownloadOutcome::unavailable(
                "no skill configuration found for this dataset source",
            ));
        };

        match self.download_and_parse(skill, result, plan).await {
            Ok(sample) => {
                if sample.records.is_empty() {
                    Ok(SampleDownloadOutcome::unavailable(
                        "sample parse failed: no recognizable records found",
                    ))
                } else {
                    Ok(SampleDownloadOutcome::available(sample))
                }
            }
            Err(e) => Ok(SampleDownloadOutcome::unavailable(e.to_string())),
        }
    }
}
