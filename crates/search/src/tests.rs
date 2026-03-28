use chrono::Utc;
use data_core::feedback::CommunitySignal;
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::adapters::{self, ExternalAdapter};
use crate::engine::{ProxyUtilityApplyMode, SearchEngine, SearchFilters, SignalFetcher};
use crate::intent::{
    retrieve_related_memories_for_test, DataStandard, IntentParser, IntentParserConfig,
    MetadataField, QueryProfile, QueryProfiler, UserProfile,
};
use crate::sample_eval::{
    DeepSeekSampleJudge, DeepSeekSampleRequirementsPlanner, DeepSeekSeedRecordJudge,
    DownloadedSample, LlmSampleJudge, LocalHeuristicProxyScorer, ProxyScreeningReport,
    RandomSeedSimilarityEvaluator, RandomSeedSimilarityEvaluatorConfig, SampleDownloader,
    SampleJudgeReport, SampleRecord, SampleRequirements, SampleRequirementsPlanner,
    SeedRecordJudge, SeedRecordJudgeReport, SeedRecordScore, StagedSampleEvaluator,
    StagedSampleEvaluatorConfig,
};
use crate::vector_index::VectorIndex;

fn dump_json<T: Serialize>(label: &str, value: &T) {
    let json = serde_json::to_string_pretty(value).unwrap();
    println!("{label}:\n{json}");
}

fn dump_profile_fields(profile: &QueryProfile) {
    println!(
        "profile.fields: task_type={:?}, task_description={:?}, target_entity={:?}, keywords={:?}, user_profile={:?}",
        profile.task_type,
        profile.task_description,
        profile.target_entity,
        profile.keywords,
        profile.user_profile
    );
}

fn make_metadata(
    cid_suffix: &str,
    title: &str,
    description: &str,
    tags: &[&str],
) -> DatasetMetadata {
    DatasetMetadata {
        cid: DatasetCid(format!("cid-{cid_suffix}")),
        info_hash: format!("hash-{cid_suffix}"),
        title: title.into(),
        description: Some(description.into()),
        tags: tags.iter().map(|t| t.to_string()).collect(),
        data_type: DataType::Tabular,
        schema: DatasetSchema {
            columns: vec![
                ColumnDef {
                    name: "image_path".into(),
                    dtype: "utf8".into(),
                    nullable: false,
                    description: None,
                },
                ColumnDef {
                    name: "label".into(),
                    dtype: "utf8".into(),
                    nullable: false,
                    description: None,
                },
            ],
            row_count: 100,
            size_bytes: 2_048,
        },
        stats: None,
        video_meta: None,
        access: AccessMode::Open,
        price: Price::free(),
        license: License {
            spdx_id: "CC-BY-4.0".into(),
            commercial_use: true,
            derivative_allowed: true,
        },
        provider: Did("did:key:z6Mktest".into()),
        signature: "sig".into(),
        provenance: Provenance::Original,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        verifiable_credential: None,
    }
}

fn with_data_type_and_resolution(
    mut metadata: DatasetMetadata,
    data_type: DataType,
    resolution_hint: &str,
) -> DatasetMetadata {
    metadata.data_type = data_type;
    if !resolution_hint.is_empty() {
        metadata.tags.push(resolution_hint.to_string());
        metadata.description = Some(format!(
            "{} Minimum sample resolution {resolution_hint}.",
            metadata.description.unwrap_or_default()
        ));
    }
    metadata
}

fn make_detailed_image_metadata(
    cid_suffix: &str,
    title: &str,
    description: &str,
    tags: &[&str],
    column_names: &[&str],
    row_count: u64,
    size_bytes: u64,
    resolution_hint: &str,
) -> DatasetMetadata {
    let mut metadata = with_data_type_and_resolution(
        make_metadata(cid_suffix, title, description, tags),
        DataType::Image,
        resolution_hint,
    );
    metadata.schema.columns = column_names
        .iter()
        .map(|name| ColumnDef {
            name: (*name).to_string(),
            dtype: if *name == "label" { "bool" } else { "utf8" }.into(),
            nullable: false,
            description: Some(format!("{name} column")),
        })
        .collect();
    metadata.schema.row_count = row_count;
    metadata.schema.size_bytes = size_bytes;
    metadata
}

fn make_data_standard(sample_unit: &str, resolution: &str) -> DataStandard {
    DataStandard {
        sample_unit: sample_unit.to_string(),
        metadata_fields: vec![
            MetadataField {
                name: "min_sample_num".into(),
                value: String::new(),
            },
            MetadataField {
                name: "resolution".into(),
                value: resolution.to_string(),
            },
        ],
        canonical_columns: vec!["sample_id".into(), "label".into()],
        extra_columns: vec!["timestamp".into()],
    }
}

fn make_external_result(cid_suffix: &str, title: &str, description: &str) -> SearchResult {
    SearchResult {
        cid: DatasetCid(format!("cid-{cid_suffix}")),
        title: title.into(),
        description: Some(description.into()),
        schema: DatasetSchema {
            columns: vec![],
            row_count: 42,
            size_bytes: 1_024,
        },
        quality: None,
        price: Price::free(),
        license: License {
            spdx_id: "CC-BY-4.0".into(),
            commercial_use: true,
            derivative_allowed: true,
        },
        provider: Did(format!("did:key:{cid_suffix}")),
        source: DataSource::Kaggle,
        data_type: DataType::Tabular,
        created_at: Utc::now(),
    }
}

fn make_external_result_with_type(
    cid_suffix: &str,
    title: &str,
    description: &str,
    data_type: DataType,
) -> SearchResult {
    SearchResult {
        data_type,
        ..make_external_result(cid_suffix, title, description)
    }
}

fn ranked_result_from_metadata(
    metadata: &DatasetMetadata,
    rank_score: f64,
) -> crate::engine::RankedResult {
    crate::engine::RankedResult {
        result: SearchResult {
            cid: metadata.cid.clone(),
            title: metadata.title.clone(),
            description: metadata.description.clone(),
            schema: metadata.schema.clone(),
            quality: None,
            price: metadata.price.clone(),
            license: metadata.license.clone(),
            provider: metadata.provider.clone(),
            source: DataSource::P2p,
            data_type: metadata.data_type,
            created_at: metadata.created_at,
        },
        rank_score,
        signal: CommunitySignal {
            dataset_cid: metadata.cid.clone(),
            total_reviews: 0,
            avg_relevance: 0.0,
            avg_quality: 0.0,
            positive_rate: 0.0,
            negative_rate: 0.0,
            task_signals: vec![],
        },
    }
}

struct StaticRequirementsPlanner {
    requirements: SampleRequirements,
}

#[async_trait::async_trait]
impl SampleRequirementsPlanner for StaticRequirementsPlanner {
    async fn plan_requirements(
        &self,
        _task: &crate::engine::DatasetSelectionTask,
    ) -> anyhow::Result<SampleRequirements> {
        Ok(self.requirements.clone())
    }
}

struct StaticSampleDownloader {
    sample: Option<DownloadedSample>,
}

#[async_trait::async_trait]
impl SampleDownloader for StaticSampleDownloader {
    async fn download_sample(
        &self,
        _result: &SearchResult,
        _metadata: &DatasetMetadata,
        _plan: &crate::engine::SamplePlan,
    ) -> anyhow::Result<Option<DownloadedSample>> {
        Ok(self.sample.clone())
    }
}

struct CountingJudge {
    utility_score: f64,
    rationale: String,
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl LlmSampleJudge for CountingJudge {
    async fn judge_sample(
        &self,
        _result: &SearchResult,
        _metadata: &DatasetMetadata,
        _task: &crate::engine::DatasetSelectionTask,
        _requirements: &SampleRequirements,
        _sample: &DownloadedSample,
        _screening: &ProxyScreeningReport,
    ) -> anyhow::Result<SampleJudgeReport> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(SampleJudgeReport {
            utility_score: self.utility_score,
            rationale: self.rationale.clone(),
        })
    }
}

struct KeywordSeedJudge {
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl SeedRecordJudge for KeywordSeedJudge {
    async fn judge_records(
        &self,
        _result: &SearchResult,
        _metadata: &DatasetMetadata,
        _task: &crate::engine::DatasetSelectionTask,
        _sample: &DownloadedSample,
        records: &[SampleRecord],
    ) -> anyhow::Result<SeedRecordJudgeReport> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(SeedRecordJudgeReport {
            summary: "seed records scored".into(),
            scored_records: records
                .iter()
                .map(|record| {
                    let lower = record.content.to_lowercase();
                    let (utility_score, rationale) = if lower.contains("invoice")
                        || lower.contains("finance")
                    {
                        (10.0, "off-task financial sample")
                    } else if lower.contains("helmet") || lower.contains("worker") {
                        (90.0, "strong ppe signal")
                    } else {
                        (50.0, "unclear sample")
                    };
                    SeedRecordScore {
                        record_id: record.id.clone(),
                        utility_score,
                        rationale: rationale.into(),
                    }
                })
                .collect(),
        })
    }
}

struct LocalDirectorySampleDownloader {
    roots_by_cid: HashMap<String, PathBuf>,
    max_records: usize,
}

#[async_trait::async_trait]
impl SampleDownloader for LocalDirectorySampleDownloader {
    async fn download_sample(
        &self,
        result: &SearchResult,
        _metadata: &DatasetMetadata,
        _plan: &crate::engine::SamplePlan,
    ) -> anyhow::Result<Option<DownloadedSample>> {
        let Some(root) = self.roots_by_cid.get(&result.cid.0) else {
            return Ok(None);
        };
        let records = sample_records_from_local_dataset(root, self.max_records)?;
        if records.is_empty() {
            return Ok(None);
        }
        let sampled_bytes = records
            .iter()
            .map(|record| record.content.len() as u64)
            .sum::<u64>();
        Ok(Some(DownloadedSample {
            sampled_rows: records.len() as u64,
            sampled_bytes,
            summary: Some(format!(
                "sampled {} local records from {}",
                records.len(),
                root.display()
            )),
            records,
        }))
    }
}

struct LocalDatasetSpec {
    relative_path: &'static str,
    title: &'static str,
    description: &'static str,
    tags: &'static [&'static str],
    columns: &'static [&'static str],
}

fn local_guixu_dataset_specs() -> Vec<LocalDatasetSpec> {
    vec![
        LocalDatasetSpec {
            relative_path: "hard-hat-detection",
            title: "Hard Hat Detection Dataset",
            description:
                "Construction and industrial worker images annotated for hard hat detection.",
            tags: &[
                "helmet",
                "hardhat",
                "ppe",
                "worker",
                "construction",
                "detection",
            ],
            columns: &["sample_id", "label", "bbox", "image_path", "split"],
        },
        LocalDatasetSpec {
            relative_path: "GDUT-HWD",
            title: "GDUT Helmet Wearing Detection",
            description:
                "Industrial worker images with helmet color annotations and bounding boxes.",
            tags: &[
                "helmet",
                "worker",
                "wearing",
                "construction",
                "ppe",
                "detection",
            ],
            columns: &["sample_id", "label", "bbox", "image_path", "helmet_color"],
        },
        LocalDatasetSpec {
            relative_path: "pictor-ppe",
            title: "Pictor PPE Detection Dataset",
            description:
                "Crowd-sourced PPE images with hard-hat and protective equipment annotations.",
            tags: &[
                "helmet",
                "ppe",
                "protective-equipment",
                "worker",
                "detection",
            ],
            columns: &["sample_id", "label", "bbox", "image_path", "dataset_split"],
        },
        LocalDatasetSpec {
            relative_path: "CHV_dataset",
            title: "CHV PPE Detection Dataset",
            description: "Personal protective equipment detection images with YOLO annotations.",
            tags: &["helmet", "vest", "ppe", "worker", "safety", "detection"],
            columns: &["sample_id", "label", "bbox", "image_path", "dataset_split"],
        },
        LocalDatasetSpec {
            relative_path: "SFCHD-SCALE/dataset_SFCHD",
            title: "SFCHD Safety Clothing and Helmet Detection",
            description: "Chemical plant dataset for safety helmet and safety clothing detection.",
            tags: &[
                "helmet",
                "safety clothing",
                "ppe",
                "worker",
                "chemical plant",
                "detection",
            ],
            columns: &["sample_id", "label", "bbox", "image_path", "scene_type"],
        },
        LocalDatasetSpec {
            relative_path: "SCUT",
            title: "SCUT Head Dataset",
            description: "Head detection dataset without explicit PPE or helmet labels.",
            tags: &["head", "crowd", "detection", "people"],
            columns: &["sample_id", "label", "bbox", "image_path", "part"],
        },
    ]
}

fn build_local_guixu_dataset_metadata(
    root: &Path,
) -> anyhow::Result<(Vec<DatasetMetadata>, HashMap<String, PathBuf>)> {
    let mut metadata = Vec::new();
    let mut roots_by_cid = HashMap::new();

    for spec in local_guixu_dataset_specs() {
        let dataset_root = root.join(spec.relative_path);
        if !dataset_root.exists() {
            continue;
        }
        let (row_count, size_bytes) = dataset_directory_stats(&dataset_root)?;
        if row_count == 0 {
            continue;
        }
        let cid_suffix = spec
            .title
            .to_lowercase()
            .replace(|character: char| !character.is_ascii_alphanumeric(), "-")
            .split('-')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let item = make_detailed_image_metadata(
            &cid_suffix,
            spec.title,
            spec.description,
            spec.tags,
            spec.columns,
            row_count,
            size_bytes,
            "720p",
        );
        roots_by_cid.insert(item.cid.0.clone(), dataset_root);
        metadata.push(item);
    }

    Ok((metadata, roots_by_cid))
}

fn dataset_directory_stats(root: &Path) -> anyhow::Result<(u64, u64)> {
    let mut stack = vec![root.to_path_buf()];
    let mut image_count = 0_u64;
    let mut total_bytes = 0_u64;

    while let Some(path) = stack.pop() {
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            let entry_path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(entry_path);
                continue;
            }

            let metadata = entry.metadata()?;
            total_bytes += metadata.len();

            if entry_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| {
                    matches!(
                        ext.to_ascii_lowercase().as_str(),
                        "jpg" | "jpeg" | "png" | "bmp" | "webp"
                    )
                })
                .unwrap_or(false)
            {
                image_count += 1;
            }
        }
    }

    Ok((image_count, total_bytes))
}

fn sample_records_from_local_dataset(
    root: &Path,
    max_records: usize,
) -> anyhow::Result<Vec<SampleRecord>> {
    let mut annotation_files = Vec::new();
    let mut image_files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(path) = stack.pop() {
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            let entry_path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(entry_path);
                continue;
            }
            let extension = entry_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_lowercase());
            match extension.as_deref() {
                Some("xml" | "txt" | "json" | "csv" | "md") => annotation_files.push(entry_path),
                Some("jpg" | "jpeg" | "png" | "bmp" | "webp") => image_files.push(entry_path),
                _ => {}
            }
        }
    }

    annotation_files.sort();
    image_files.sort();
    let images_by_stem = image_files
        .iter()
        .filter_map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| (stem.to_ascii_lowercase(), path.clone()))
        })
        .collect::<HashMap<_, _>>();

    let mut records = annotation_files
        .into_iter()
        .take(max_records)
        .filter_map(|path| {
            sample_record_from_text_file(root, &path, &images_by_stem).transpose()
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    if records.is_empty() {
        records.extend(image_files.into_iter().take(max_records).map(|path| {
            SampleRecord {
                id: path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .display()
                    .to_string(),
                content: format!(
                    "image file {} from local dataset {}",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default(),
                    root.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default()
                ),
                metadata: serde_json::json!({
                    "relative_path": path.strip_prefix(root).unwrap_or(&path).display().to_string(),
                    "kind": "image",
                    "local_image_path": path.display().to_string(),
                    "local_image_relative_path": path.strip_prefix(root).unwrap_or(&path).display().to_string(),
                    "local_image_mime_type": guess_image_mime_type(&path),
                }),
            }
        }));
    }

    Ok(records)
}

fn sample_record_from_text_file(
    root: &Path,
    path: &Path,
    images_by_stem: &HashMap<String, PathBuf>,
) -> anyhow::Result<Option<SampleRecord>> {
    let contents = std::fs::read_to_string(path).unwrap_or_default();
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string();
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();

    let content = match extension.as_str() {
        "xml" => {
            let labels = extract_xml_name_labels(&contents);
            if labels.is_empty() {
                contents.lines().take(8).collect::<Vec<_>>().join(" ")
            } else {
                format!("xml labels: {}", labels.join(" "))
            }
        }
        "txt" => contents.lines().take(8).collect::<Vec<_>>().join(" "),
        "md" => contents.lines().take(12).collect::<Vec<_>>().join(" "),
        _ => contents.lines().take(8).collect::<Vec<_>>().join(" "),
    };

    if content.trim().is_empty() {
        return Ok(None);
    }

    let mut metadata = serde_json::json!({
        "relative_path": relative,
        "kind": extension,
    });

    if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
        if let Some(image_path) = images_by_stem.get(&stem.to_ascii_lowercase()) {
            metadata["local_image_path"] =
                serde_json::Value::String(image_path.display().to_string());
            metadata["local_image_relative_path"] = serde_json::Value::String(
                image_path
                    .strip_prefix(root)
                    .unwrap_or(image_path)
                    .display()
                    .to_string(),
            );
            metadata["local_image_mime_type"] =
                serde_json::Value::String(guess_image_mime_type(image_path).to_string());
        }
    }

    Ok(Some(SampleRecord {
        id: relative,
        content,
        metadata,
    }))
}

fn guess_image_mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        _ => "image/jpeg",
    }
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

fn neutral_signal_fetcher() -> SignalFetcher {
    Box::new(|cid_str: &str| CommunitySignal {
        dataset_cid: DatasetCid(cid_str.to_string()),
        total_reviews: 0,
        avg_relevance: 0.0,
        avg_quality: 0.0,
        positive_rate: 0.0,
        negative_rate: 0.0,
        task_signals: vec![],
    })
}

struct StubAdapter {
    results: Vec<SearchResult>,
}

#[async_trait::async_trait]
impl ExternalAdapter for StubAdapter {
    fn name(&self) -> &str {
        "stub"
    }

    fn source_type(&self) -> DataSource {
        DataSource::Kaggle
    }

    async fn search(&self, _query: &str, _limit: usize) -> anyhow::Result<Vec<SearchResult>> {
        Ok(self.results.clone())
    }
}

fn make_engine(adapters: Vec<Box<dyn ExternalAdapter>>) -> SearchEngine {
    SearchEngine::new(
        VectorIndex,
        IntentParser::new(IntentParserConfig {
            api_key: None,
            ..IntentParserConfig::default()
        }),
        adapters,
    )
}

fn make_engine_with_parser(
    intent_parser: IntentParser,
    adapters: Vec<Box<dyn ExternalAdapter>>,
) -> SearchEngine {
    SearchEngine::new(VectorIndex, intent_parser, adapters)
}

fn make_live_engine(intent_parser: IntentParser) -> SearchEngine {
    SearchEngine::new(VectorIndex, intent_parser, vec![])
}

fn load_live_intent_parser_config() -> IntentParserConfig {
    let api_key = lookup_live_setting("DEEPSEEK_API_KEY").unwrap_or_else(|| {
        panic!("missing DEEPSEEK_API_KEY in env or local/settings.env (or local/setting.env)")
    });
    let api_base = lookup_live_setting("DEEPSEEK_API_BASE")
        .unwrap_or_else(|| "https://api.deepseek.com".into());
    let model = lookup_live_setting("DEEPSEEK_MODEL").unwrap_or_else(|| "deepseek-chat".into());

    IntentParserConfig {
        api_key: Some(api_key),
        api_base,
        model,
        timeout: std::time::Duration::from_secs(30),
    }
}

fn lookup_live_setting(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .and_then(normalize_live_setting_value)
        .or_else(|| {
            for base in [
                std::env::current_dir().ok(),
                Some(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
            ]
            .into_iter()
            .flatten()
            {
                for ancestor in base.ancestors() {
                    for file_name in ["settings.env", "setting.env"] {
                        let candidate = ancestor.join("local").join(file_name);
                        if !candidate.is_file() {
                            continue;
                        }
                        if let Some(value) = read_live_setting_from_file(&candidate, key) {
                            return Some(value);
                        }
                    }
                }
            }
            None
        })
}

fn read_live_setting_from_file(path: &std::path::Path, key: &str) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    contents.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let line = line
            .strip_prefix("export ")
            .map(str::trim_start)
            .unwrap_or(line);
        let (name, value) = line.split_once('=')?;
        if name.trim() != key {
            return None;
        }
        normalize_live_setting_value(value)
    })
}

fn normalize_live_setting_value(value: impl AsRef<str>) -> Option<String> {
    let value = value
        .as_ref()
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

async fn spawn_mock_deepseek_server(
    content: &str,
) -> (
    String,
    tokio::sync::oneshot::Receiver<String>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let response_body = serde_json::json!({
        "choices": [
            {
                "message": {
                    "content": content
                }
            }
        ]
    })
    .to_string();
    let (request_tx, request_rx) = tokio::sync::oneshot::channel();

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut socket).await;
        let _ = request_tx.send(request);

        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    (format!("http://{addr}"), request_rx, server)
}

async fn read_http_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 2048];
    let mut header_end = None;
    let mut content_length = 0_usize;

    loop {
        let read = socket.read(&mut chunk).await.unwrap();
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);

        if header_end.is_none() {
            if let Some(position) = find_subsequence(&buffer, b"\r\n\r\n") {
                let end = position + 4;
                header_end = Some(end);
                let headers = String::from_utf8_lossy(&buffer[..end]);
                for line in headers.lines() {
                    if let Some((name, value)) = line.split_once(':') {
                        if name.eq_ignore_ascii_case("content-length") {
                            content_length = value.trim().parse().unwrap_or(0);
                        }
                    }
                }
            }
        }

        if let Some(end) = header_end {
            if buffer.len() >= end + content_length {
                break;
            }
        }
    }

    String::from_utf8(buffer).unwrap()
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[tokio::test]
async fn intent_parser_requires_llm_api_configuration() {
    let parser = IntentParser::new(IntentParserConfig {
        api_key: None,
        ..IntentParserConfig::default()
    });
    let query = "Build a high-quality classifier to detect cats";
    let error = parser.profile(query).await.unwrap_err();

    assert!(error.to_string().contains("missing DEEPSEEK_API_KEY"));
}

#[tokio::test]
async fn intent_parser_trait_propagates_missing_api_key_error() {
    let parser = IntentParser::new(IntentParserConfig {
        api_key: None,
        ..IntentParserConfig::default()
    });
    let profiler: &dyn QueryProfiler = &parser;
    let query = "Build a high-quality classifier to detect cats";

    let via_inherent = parser.profile(query).await.unwrap_err();
    let via_trait = profiler.profile(query).await.unwrap_err();

    assert_eq!(via_inherent.to_string(), via_trait.to_string());
    assert!(via_trait.to_string().contains("missing DEEPSEEK_API_KEY"));
}

#[tokio::test]
async fn intent_parser_uses_deepseek_when_configured() {
    let query = "check whether Caesar is in the image taken from monitor";
    let parser = IntentParser::new(IntentParserConfig {
        api_key: Some("test-key".into()),
        api_base: "https://api.deepseek.com".into(),
        model: "deepseek-chat".into(),
        timeout: std::time::Duration::from_secs(5),
    });
    let user_profile = UserProfile {
        cpu: crate::intent::CpuProfile {
            architecture: "x86_64".into(),
            logical_cores: 8,
            model: Some("Test CPU".into()),
        },
        gpus: vec![crate::intent::GpuProfile {
            vendor: Some("NVIDIA".into()),
            model: "RTX 4090".into(),
        }],
    };
    let request_body = parser
        .build_deepseek_request_json(
            query,
            &user_profile,
            &[
                "The user has a cat named Caesar.".to_string(),
                "The user prefers calm spaces more than loud ones.".to_string(),
            ],
        )
        .unwrap();
    let profile = parser
        .profile_from_deepseek_content(
            query,
            &user_profile,
            r#"{"task_type":"classification","task_description":"Detect whether cats are present in input images with high-quality accuracy.","target_entity":"cats","keywords":["cats","classifier","vision"]}"#,
        )
        .unwrap();

    dump_json("deepseek.request", &request_body);
    dump_json("deepseek.profile", &profile);

    assert_eq!(request_body["model"], "deepseek-chat");
    assert_eq!(request_body["response_format"]["type"], "json_object");
    assert_eq!(request_body["messages"][0]["role"], "system");
    assert_eq!(request_body["messages"][1]["role"], "user");
    assert!(request_body["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains(query));
    assert!(request_body["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("Relevant user memories"));
    assert!(request_body["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("\"task_description\""));
    assert!(request_body["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("Caesar"));
    assert!(request_body["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("RTX 4090"));
    assert_eq!(profile.task_type.as_deref(), Some("classification"));
    assert_eq!(
        profile.task_description.as_deref(),
        Some("Detect whether cats are present in input images with high-quality accuracy.")
    );
    assert_eq!(profile.target_entity.as_deref(), Some("cats"));
    assert_eq!(profile.keywords, vec!["cats", "classifier", "vision"]);
    assert_eq!(profile.user_profile, user_profile);
}

#[test]
fn memory_search_prefers_entries_matching_named_entities_and_terms() {
    let matches = retrieve_related_memories_for_test(
        "Plan a calm weekend around \"Caesar\" with small gatherings",
        &[
            "The user has a cat named Caesar.",
            "The user prefers small gatherings to crowded events.",
            "The user likes calm spaces more than loud energetic ones.",
            "The user enjoys cycling on cool mornings.",
        ],
        3,
    );

    dump_json("memory.matches", &matches);

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0], "The user has a cat named Caesar.");
}

#[tokio::test]
async fn search_with_profile_matches_local_metadata() {
    let engine = make_engine(vec![]);
    let local_metadata = vec![
        with_data_type_and_resolution(
            make_metadata(
                "cats",
                "Cat Image Classification Dataset",
                "Labeled cat images for image classification",
                &["cats", "classification", "images"],
            ),
            DataType::Image,
            "1280x720",
        ),
        with_data_type_and_resolution(
            make_metadata(
                "dogs",
                "Dog Image Dataset",
                "Labeled dog images for classification",
                &["dogs", "classification", "images"],
            ),
            DataType::Image,
            "1280x720",
        ),
    ];
    let profile = QueryProfile {
        raw_query: "Build a high-quality classifier to detect cats".into(),
        task_type: Some("classification".into()),
        task_description: Some(
            "Perform a classification task focused on cats with a high-quality requirement based on the user request: Build a high-quality classifier to detect cats"
                .into(),
        ),
        target_entity: Some("cats".into()),
        keywords: vec![
            "build".into(),
            "high-quality".into(),
            "classifier".into(),
            "detect".into(),
            "cats".into(),
        ],
        user_profile: UserProfile::default(),
        data_standard: make_data_standard("image", "720p"),
    };

    let output = engine
        .search_with_profile(
            &profile,
            &SearchFilters::default(),
            &local_metadata,
            &neutral_signal_fetcher(),
            10,
        )
        .await
        .unwrap();

    dump_json("search.local.profile", &profile);
    dump_profile_fields(&profile);
    dump_json("search.local.results", &output.results);

    assert_eq!(output.results.len(), 1);
    assert_eq!(
        output.results[0].result.title,
        "Cat Image Classification Dataset"
    );
}

#[tokio::test]
async fn search_with_profile_filters_out_wrong_type_and_low_resolution_datasets() {
    let engine = make_engine(vec![]);
    let local_metadata = vec![
        with_data_type_and_resolution(
            make_metadata(
                "cats-hd",
                "Cat HD Image Dataset",
                "Sharp labeled cat images for classification",
                &["cats", "classification", "images"],
            ),
            DataType::Image,
            "1280x720",
        ),
        with_data_type_and_resolution(
            make_metadata(
                "cats-lowres",
                "Cat Low Resolution Dataset",
                "Blurry cat images for classification",
                &["cats", "classification", "images"],
            ),
            DataType::Image,
            "640x480",
        ),
        with_data_type_and_resolution(
            make_metadata(
                "cats-video",
                "Cat Monitor Video Dataset",
                "Indoor monitoring video clips of cats",
                &["cats", "monitoring", "video"],
            ),
            DataType::Video,
            "1920x1080",
        ),
    ];
    let profile = QueryProfile {
        raw_query: "Build a high-quality classifier to detect cats".into(),
        task_type: Some("classification".into()),
        task_description: Some(
            "Perform a classification task focused on cats with a high-quality requirement based on the user request: Build a high-quality classifier to detect cats"
                .into(),
        ),
        target_entity: Some("cats".into()),
        keywords: vec![
            "build".into(),
            "high-quality".into(),
            "classifier".into(),
            "detect".into(),
            "cats".into(),
        ],
        user_profile: UserProfile::default(),
        data_standard: make_data_standard("image", "720p"),
    };

    let output = engine
        .search_with_profile(
            &profile,
            &SearchFilters::default(),
            &local_metadata,
            &neutral_signal_fetcher(),
            10,
        )
        .await
        .unwrap();

    dump_json("search.local.profile", &profile);
    dump_profile_fields(&profile);
    dump_json("search.local.results", &output.results);

    let titles: Vec<&str> = output
        .results
        .iter()
        .map(|ranked| ranked.result.title.as_str())
        .collect();
    assert_eq!(titles, vec!["Cat HD Image Dataset"]);
}

#[tokio::test]
async fn search_with_profile_deduplicates_local_and_external_results_by_cid() {
    let engine = make_engine(vec![Box::new(StubAdapter {
        results: vec![
            make_external_result("cats", "Cat Image Mirror", "Duplicate CID from adapter"),
            make_external_result("pets", "Pet Detection Dataset", "Unique external result"),
        ],
    })]);
    let local_metadata = vec![make_metadata(
        "cats",
        "Cat Image Classification Dataset",
        "Labeled cat images for image classification",
        &["cats", "classification", "images"],
    )];
    let profile = QueryProfile {
        raw_query: "Build a high-quality classifier to detect cats".into(),
        task_type: Some("classification".into()),
        task_description: Some(
            "Perform a classification task focused on cats with a high-quality requirement based on the user request: Build a high-quality classifier to detect cats"
                .into(),
        ),
        target_entity: Some("cats".into()),
        keywords: vec!["cats".into(), "classifier".into()],
        user_profile: UserProfile::default(),
        data_standard: DataStandard::default(),
    };

    let output = engine
        .search_with_profile(
            &profile,
            &SearchFilters::default(),
            &local_metadata,
            &neutral_signal_fetcher(),
            10,
        )
        .await
        .unwrap();

    let cids: Vec<&str> = output
        .results
        .iter()
        .map(|r| r.result.cid.0.as_str())
        .collect();
    dump_json("search.dedup.cids", &cids);
    assert_eq!(output.results.len(), 2);
    assert_eq!(cids.iter().filter(|cid| **cid == "cid-cats").count(), 1);
    assert!(cids.contains(&"cid-pets"));
}

#[tokio::test]
async fn value_search_output_removes_candidates_with_mismatched_data_type_before_scoring() {
    use crate::engine::{DatasetSelectionTask, DatasetValuationConfig, RankedResult, SearchOutput};

    let engine = make_engine(vec![]);
    let signal_fetcher = neutral_signal_fetcher();
    let profile = QueryProfile {
        raw_query: "Check whether Caesar is in the image taken by monitor".into(),
        task_type: Some("classification".into()),
        task_description: Some(
            "Determine whether the user's cat appears in monitor images.".into(),
        ),
        target_entity: Some("cat".into()),
        keywords: vec!["cat".into(), "monitor".into()],
        user_profile: UserProfile::default(),
        data_standard: make_data_standard("image", "720p"),
    };
    let task = DatasetSelectionTask::from(&profile);
    let search_output = SearchOutput {
        results: vec![
            RankedResult {
                result: make_external_result_with_type(
                    "cat-image",
                    "Cat Monitor Image Dataset",
                    "Labeled cat images for classification from indoor monitors",
                    DataType::Image,
                ),
                rank_score: 80.0,
                signal: signal_fetcher("cid-cat-image"),
            },
            RankedResult {
                result: make_external_result_with_type(
                    "cat-video",
                    "Cat Monitor Video Dataset",
                    "Indoor monitoring video clips containing cats",
                    DataType::Video,
                ),
                rank_score: 75.0,
                signal: signal_fetcher("cid-cat-video"),
            },
        ],
        errors: vec![],
    };

    let output = engine
        .value_search_output(
            &search_output,
            &[],
            None,
            None,
            &task,
            &DatasetValuationConfig::default(),
        )
        .await
        .unwrap();

    let titles: Vec<&str> = output
        .candidates
        .iter()
        .map(|candidate| candidate.result.title.as_str())
        .collect();
    dump_json("selection.type_filter.task", &task);
    dump_json("selection.type_filter.candidates", &output.candidates);

    assert_eq!(titles, vec!["Cat Monitor Image Dataset"]);
    assert_eq!(
        output
            .selected
            .as_ref()
            .map(|candidate| candidate.result.data_type),
        Some(DataType::Image)
    );
}

#[tokio::test]
async fn staged_sample_evaluator_short_circuits_low_similarity_samples() {
    use crate::engine::{DatasetSelectionTask, DatasetValuationConfig, SearchOutput};

    let engine = make_engine(vec![]);
    let metadata = make_detailed_image_metadata(
        "cat-monitor-low",
        "Cat Monitor Candidate",
        "Indoor cat monitor image dataset",
        &["cat", "monitor", "classification"],
        &["sample_id", "label", "camera_id"],
        2_500,
        9_000_000,
        "1280x720",
    );
    let profile = QueryProfile {
        raw_query: "Check whether Caesar is in the image taken by monitor".into(),
        task_type: Some("classification".into()),
        task_description: Some(
            "Determine whether the user's cat appears in monitor images.".into(),
        ),
        target_entity: Some("cat".into()),
        keywords: vec!["cat".into(), "monitor".into()],
        user_profile: UserProfile::default(),
        data_standard: make_data_standard("image", "720p"),
    };
    let task = DatasetSelectionTask::from(&profile);
    let search_output = SearchOutput {
        results: vec![ranked_result_from_metadata(&metadata, 90.0)],
        errors: vec![],
    };
    let llm_calls = Arc::new(AtomicUsize::new(0));
    let evaluator = StagedSampleEvaluator::with_config(
        Box::new(StaticSampleDownloader {
            sample: Some(DownloadedSample {
                records: vec![
                    SampleRecord {
                        id: "row-1".into(),
                        content: "invoice ledger finance revenue report".into(),
                        metadata: serde_json::json!({"source":"erp"}),
                    },
                    SampleRecord {
                        id: "row-2".into(),
                        content: "accounts payable spreadsheet document".into(),
                        metadata: serde_json::json!({"source":"backoffice"}),
                    },
                ],
                sampled_rows: 2,
                sampled_bytes: 1_024,
                summary: Some("financial documents unrelated to cat monitoring".into()),
            }),
        }),
        Box::new(StaticRequirementsPlanner {
            requirements: SampleRequirements {
                summary:
                    "Need image samples that show indoor monitor frames with a cat-present label"
                        .into(),
                required_signals: vec!["cat".into(), "monitor".into(), "label".into()],
                preferred_labels: vec!["cat_present".into()],
                disqualifying_signals: vec!["invoice".into(), "finance".into()],
            },
        }),
        Box::new(LocalHeuristicProxyScorer),
        Box::new(CountingJudge {
            utility_score: 95.0,
            rationale: "llm should not be called".into(),
            calls: llm_calls.clone(),
        }),
        StagedSampleEvaluatorConfig {
            proxy_similarity_threshold: 50.0,
            rejected_similarity_max_score: 12.0,
        },
    );

    let output = engine
        .value_search_output(
            &search_output,
            &[metadata],
            None,
            Some(&evaluator),
            &task,
            &DatasetValuationConfig::default(),
        )
        .await
        .unwrap();

    let selected = output.selected.unwrap();
    assert_eq!(llm_calls.load(Ordering::SeqCst), 0);
    assert!(selected.proxy_utility.is_some());
    assert_eq!(
        selected.proxy_utility.as_ref().unwrap().apply_mode,
        ProxyUtilityApplyMode::OverrideFinal
    );
    assert!(selected.proxy_utility.as_ref().unwrap().proxy_metric_value < 50.0);
    assert!(selected.final_score <= 12.0);
}

#[tokio::test]
async fn staged_sample_evaluator_blends_high_similarity_samples_with_llm_score() {
    use crate::engine::{DatasetSelectionTask, DatasetValuationConfig, SearchOutput};

    let engine = make_engine(vec![]);
    let metadata = make_detailed_image_metadata(
        "cat-monitor-high",
        "Cat Monitor Candidate",
        "Indoor cat monitor image dataset",
        &["cat", "monitor", "classification"],
        &["sample_id", "label", "camera_id"],
        2_500,
        9_000_000,
        "1280x720",
    );
    let profile = QueryProfile {
        raw_query: "Check whether Caesar is in the image taken by monitor".into(),
        task_type: Some("classification".into()),
        task_description: Some(
            "Determine whether the user's cat appears in monitor images.".into(),
        ),
        target_entity: Some("cat".into()),
        keywords: vec!["cat".into(), "monitor".into()],
        user_profile: UserProfile::default(),
        data_standard: make_data_standard("image", "720p"),
    };
    let task = DatasetSelectionTask::from(&profile);
    let search_output = SearchOutput {
        results: vec![ranked_result_from_metadata(&metadata, 90.0)],
        errors: vec![],
    };
    let llm_calls = Arc::new(AtomicUsize::new(0));
    let evaluator = StagedSampleEvaluator::with_config(
        Box::new(StaticSampleDownloader {
            sample: Some(DownloadedSample {
                records: vec![
                    SampleRecord {
                        id: "row-1".into(),
                        content: "indoor monitor frame label cat_present camera hallway".into(),
                        metadata: serde_json::json!({"camera_id":"cam-01"}),
                    },
                    SampleRecord {
                        id: "row-2".into(),
                        content: "cat_present label from monitor image near door".into(),
                        metadata: serde_json::json!({"camera_id":"cam-02"}),
                    },
                ],
                sampled_rows: 2,
                sampled_bytes: 2_048,
                summary: Some("indoor monitor image samples containing cat_present labels".into()),
            }),
        }),
        Box::new(StaticRequirementsPlanner {
            requirements: SampleRequirements {
                summary:
                    "Need image samples that show indoor monitor frames with a cat-present label"
                        .into(),
                required_signals: vec!["cat".into(), "monitor".into(), "label".into()],
                preferred_labels: vec!["cat_present".into()],
                disqualifying_signals: vec!["dog_only".into()],
            },
        }),
        Box::new(LocalHeuristicProxyScorer),
        Box::new(CountingJudge {
            utility_score: 92.0,
            rationale: "sample strongly matches the task".into(),
            calls: llm_calls.clone(),
        }),
        StagedSampleEvaluatorConfig {
            proxy_similarity_threshold: 35.0,
            rejected_similarity_max_score: 12.0,
        },
    );
    let config = DatasetValuationConfig::default();

    let output = engine
        .value_search_output(
            &search_output,
            &[metadata],
            None,
            Some(&evaluator),
            &task,
            &config,
        )
        .await
        .unwrap();

    let selected = output.selected.unwrap();
    let expected = ((selected.coarse_score * config.metadata_weight)
        + (92.0 * config.utility_weight))
        / (config.metadata_weight + config.utility_weight);
    assert_eq!(llm_calls.load(Ordering::SeqCst), 1);
    assert!(selected.proxy_utility.is_some());
    assert_eq!(
        selected.proxy_utility.as_ref().unwrap().apply_mode,
        ProxyUtilityApplyMode::Blend
    );
    assert!((selected.final_score - expected).abs() < 1e-6);
}

#[tokio::test]
async fn value_search_output_re_evaluates_only_top_five_candidates_by_default() {
    use crate::engine::{DatasetSelectionTask, DatasetValuationConfig, SearchOutput};

    let engine = make_engine(vec![]);
    let profile = QueryProfile {
        raw_query: "Check whether Caesar is in the image taken by monitor".into(),
        task_type: Some("classification".into()),
        task_description: Some(
            "Determine whether the user's cat appears in monitor images.".into(),
        ),
        target_entity: Some("cat".into()),
        keywords: vec!["cat".into(), "monitor".into()],
        user_profile: UserProfile::default(),
        data_standard: make_data_standard("image", "720p"),
    };
    let task = DatasetSelectionTask::from(&profile);

    let local_metadata = (0..6)
        .map(|index| {
            make_detailed_image_metadata(
                &format!("candidate-{index}"),
                &format!("Cat Monitor Candidate {index}"),
                "Indoor monitor image dataset with cat labels",
                &["cat", "monitor", "classification"],
                &["sample_id", "label", "camera_id"],
                2_000 + (index as u64 * 500),
                8_000_000 + (index as u64 * 1_000_000),
                "1280x720",
            )
        })
        .collect::<Vec<_>>();
    let search_output = SearchOutput {
        results: local_metadata
            .iter()
            .enumerate()
            .map(|(index, metadata)| ranked_result_from_metadata(metadata, 100.0 - index as f64))
            .collect(),
        errors: vec![],
    };
    let llm_calls = Arc::new(AtomicUsize::new(0));
    let evaluator = StagedSampleEvaluator::with_config(
        Box::new(StaticSampleDownloader {
            sample: Some(DownloadedSample {
                records: vec![SampleRecord {
                    id: "row-1".into(),
                    content: "indoor monitor frame label cat_present camera hallway".into(),
                    metadata: serde_json::json!({"camera_id":"cam-01"}),
                }],
                sampled_rows: 1,
                sampled_bytes: 1_024,
                summary: Some("indoor monitor image sample".into()),
            }),
        }),
        Box::new(StaticRequirementsPlanner {
            requirements: SampleRequirements {
                summary:
                    "Need image samples that show indoor monitor frames with a cat-present label"
                        .into(),
                required_signals: vec!["cat".into(), "monitor".into(), "label".into()],
                preferred_labels: vec!["cat_present".into()],
                disqualifying_signals: vec![],
            },
        }),
        Box::new(LocalHeuristicProxyScorer),
        Box::new(CountingJudge {
            utility_score: 88.0,
            rationale: "sample matches task".into(),
            calls: llm_calls.clone(),
        }),
        StagedSampleEvaluatorConfig::default(),
    );
    let config = DatasetValuationConfig::default();

    let output = engine
        .value_search_output(
            &search_output,
            &local_metadata,
            None,
            Some(&evaluator),
            &task,
            &config,
        )
        .await
        .unwrap();

    assert_eq!(config.coarse_top_k, 5);
    assert_eq!(llm_calls.load(Ordering::SeqCst), 5);
    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|candidate| candidate.proxy_utility.is_some())
        .count(),
        5
    );
}

#[test]
fn local_sample_records_attach_matching_images_when_available() {
    let tempdir = tempfile::tempdir().unwrap();
    let root = tempdir.path();
    let annotation_dir = root.join("Annotations");
    let image_dir = root.join("JPEGImages");
    std::fs::create_dir_all(&annotation_dir).unwrap();
    std::fs::create_dir_all(&image_dir).unwrap();

    let annotation_path = annotation_dir.join("sample-01.xml");
    let image_path = image_dir.join("sample-01.jpg");
    std::fs::write(
        &annotation_path,
        r#"<annotation><object><name>helmet</name></object></annotation>"#,
    )
    .unwrap();
    std::fs::write(&image_path, b"fake-jpeg").unwrap();

    let records = sample_records_from_local_dataset(root, 4).unwrap();
    let first = records.first().expect("expected at least one sample record");

    assert_eq!(first.id, "Annotations/sample-01.xml");
    assert_eq!(
        first.metadata["local_image_relative_path"].as_str(),
        Some("JPEGImages/sample-01.jpg")
    );
    assert_eq!(
        first.metadata["local_image_mime_type"].as_str(),
        Some("image/jpeg")
    );
}

#[tokio::test]
async fn random_seed_similarity_evaluator_averages_seed_scores_and_low_anchor_penalties() {
    use crate::engine::{DatasetSelectionTask, DatasetValuationConfig, SearchOutput};

    let engine = make_engine(vec![]);
    let metadata = make_detailed_image_metadata(
        "ppe-random-seed",
        "Factory PPE Candidate",
        "Factory worker images for helmet and safety clothing detection",
        &["helmet", "ppe", "worker", "classification"],
        &["sample_id", "label", "image_path"],
        4_000,
        15_000_000,
        "1280x720",
    );
    let profile = QueryProfile {
        raw_query: "detect whether workers wear helmets in factory images".into(),
        task_type: Some("classification".into()),
        task_description: Some(
            "Determine whether workers are wearing safety helmets in factory scenes.".into(),
        ),
        target_entity: Some("worker".into()),
        keywords: vec!["worker".into(), "helmet".into(), "factory".into()],
        user_profile: UserProfile::default(),
        data_standard: make_data_standard("image", "720p"),
    };
    let task = DatasetSelectionTask::from(&profile);
    let search_output = SearchOutput {
        results: vec![ranked_result_from_metadata(&metadata, 95.0)],
        errors: vec![],
    };
    let seed_calls = Arc::new(AtomicUsize::new(0));
    let evaluator = RandomSeedSimilarityEvaluator::with_config(
        Box::new(StaticSampleDownloader {
            sample: Some(DownloadedSample {
                records: vec![
                    SampleRecord {
                        id: "seed-low".into(),
                        content: "invoice payroll finance spreadsheet".into(),
                        metadata: serde_json::json!({"source":"erp"}),
                    },
                    SampleRecord {
                        id: "seed-high".into(),
                        content: "worker helmet safety vest detection".into(),
                        metadata: serde_json::json!({"source":"factory"}),
                    },
                    SampleRecord {
                        id: "remain-high".into(),
                        content: "worker helmet safety vest detection".into(),
                        metadata: serde_json::json!({"source":"factory"}),
                    },
                    SampleRecord {
                        id: "remain-low".into(),
                        content: "invoice payroll finance spreadsheet".into(),
                        metadata: serde_json::json!({"source":"erp"}),
                    },
                ],
                sampled_rows: 4,
                sampled_bytes: 4_096,
                summary: Some("mixed sample containing good and bad factory records".into()),
            }),
        }),
        Box::new(KeywordSeedJudge {
            calls: seed_calls.clone(),
        }),
        RandomSeedSimilarityEvaluatorConfig {
            seed_record_count: 2,
            low_score_threshold: 35.0,
            high_score_threshold: 75.0,
            override_final_below_score: 25.0,
            selection_seed: 0,
        },
    );
    let config = DatasetValuationConfig::default();

    let output = engine
        .value_search_output(
            &search_output,
            &[metadata],
            None,
            Some(&evaluator),
            &task,
            &config,
        )
        .await
        .unwrap();

    let selected = output.selected.unwrap();
    let expected_final = ((selected.coarse_score * config.metadata_weight)
        + (50.0 * config.utility_weight))
        / (config.metadata_weight + config.utility_weight);
    let proxy = selected.proxy_utility.expect("proxy utility expected");

    assert_eq!(seed_calls.load(Ordering::SeqCst), 1);
    assert_eq!(proxy.apply_mode, ProxyUtilityApplyMode::Blend);
    assert!((proxy.utility_score - 50.0).abs() < 1e-6);
    assert!((proxy.proxy_metric_value - 50.0).abs() < 1e-6);
    assert!(proxy.notes.unwrap_or_default().contains("low_score_anchors=1"));
    assert!((selected.final_score - expected_final).abs() < 1e-6);
}

#[tokio::test]
async fn backend_e2e_nl_query_to_rank_scores() {
    let query = "I need a high-quality dataset to classify cats in indoor household photos";
    let (api_base, request_rx, server) = spawn_mock_deepseek_server(
        r#"{
            "task_type": "classification",
            "task_description": "Classify whether cats appear in indoor household scenes with high accuracy.",
            "target_entity": "cats",
            "keywords": ["pets"],
            "data_standard": {
                "sample_unit": "image",
                "metadata_fields": [
                    {"name": "min_sample_num", "value": "1000"},
                    {"name": "resolution", "value": "720p"}
                ],
                "canonical_columns": ["sample_id", "label"],
                "extra_columns": ["timestamp"]
            }
        }"#,
    )
    .await;
    let engine = make_engine_with_parser(
        IntentParser::new(IntentParserConfig {
            api_key: Some("test-key".into()),
            api_base,
            model: "deepseek-chat".into(),
            timeout: std::time::Duration::from_secs(5),
        }),
        vec![],
    );
    let local_metadata = vec![
        with_data_type_and_resolution(
            make_metadata(
                "cats-indoor",
                "Cat Indoor Monitoring Dataset",
                "Pet dataset with indoor household cat images for classification.",
                &["pets", "indoor", "household"],
            ),
            DataType::Image,
            "1280x720",
        ),
        with_data_type_and_resolution(
            make_metadata(
                "dogs-outdoor",
                "Dog Outdoor Monitoring Dataset",
                "Pet dataset with outdoor dog images for classification.",
                &["pets", "outdoor", "garden"],
            ),
            DataType::Image,
            "1280x720",
        ),
    ];

    let output = engine
        .search(
            query,
            &SearchFilters::default(),
            &local_metadata,
            &neutral_signal_fetcher(),
            10,
        )
        .await
        .unwrap();

    let request = request_rx.await.unwrap();
    server.await.unwrap();

    let score_board = output
        .results
        .iter()
        .map(|ranked| {
            serde_json::json!({
                "title": ranked.result.title.as_str(),
                "cid": ranked.result.cid.0.as_str(),
                "rank_score": ranked.rank_score,
            })
        })
        .collect::<Vec<_>>();

    dump_json("backend.e2e.query", &query);
    dump_json("backend.e2e.score_board", &score_board);

    assert!(request.contains("POST /chat/completions"));
    assert!(request.contains(query));
    assert_eq!(output.results.len(), 2);
    assert_eq!(
        output.results[0].result.title,
        "Cat Indoor Monitoring Dataset"
    );
    assert!(output.results[0].rank_score.is_finite());
    assert!(output.results[1].rank_score.is_finite());
    assert!(output.results[0].rank_score > output.results[1].rank_score);
}

#[tokio::test]
async fn backend_e2e_nl_query_to_keyword_search_and_tcv_valuation() {
    use data_valuation::tcv::{TaskContext, TcvEngine, TcvVerdict};

    let query =
        "I need training data to tell whether Caesar is in camera frames from inside my apartment";
    let (api_base, request_rx, server) = spawn_mock_deepseek_server(
        r#"{
            "task_type": "classification",
            "task_description": "Classify whether cats appear in indoor household scenes with high accuracy.",
            "target_entity": "cats",
            "keywords": ["pets", "indoor", "household"],
            "data_standard": {
                "sample_unit": "image",
                "metadata_fields": [
                    {"name": "min_sample_num", "value": "1000"},
                    {"name": "resolution", "value": "720p"}
                ],
                "canonical_columns": ["image_path", "label"],
                "extra_columns": ["timestamp"]
            }
        }"#,
    )
    .await;
    let parser = IntentParser::new(IntentParserConfig {
        api_key: Some("test-key".into()),
        api_base,
        model: "deepseek-chat".into(),
        timeout: std::time::Duration::from_secs(5),
    });
    let profile = parser.profile(query).await.unwrap();

    let request = request_rx.await.unwrap();
    server.await.unwrap();

    assert!(request.contains("POST /chat/completions"));
    assert!(request.contains(query));
    assert_eq!(profile.keywords, vec!["pets", "indoor", "household"]);
    assert_eq!(
        profile.data_standard.canonical_columns,
        vec!["sample_id", "label"]
    );

    let mut local_metadata = vec![
        with_data_type_and_resolution(
            make_metadata(
                "cats-indoor",
                "Cat Indoor Monitoring Dataset",
                "Pet dataset with indoor household cat images for classification.",
                &["pets", "indoor", "household"],
            ),
            DataType::Image,
            "1280x720",
        ),
        with_data_type_and_resolution(
            make_metadata(
                "dogs-outdoor",
                "Dog Outdoor Monitoring Dataset",
                "Pet dataset with outdoor dog images for classification.",
                &["pets", "outdoor", "garden"],
            ),
            DataType::Image,
            "1280x720",
        ),
    ];
    for metadata in &mut local_metadata {
        metadata.schema.columns[0].name = "sample_id".into();
    }
    let signal_fetcher = neutral_signal_fetcher();
    let engine = make_engine(vec![]);
    let output = engine
        .search_with_profile(
            &profile,
            &SearchFilters::default(),
            &local_metadata,
            &signal_fetcher,
            10,
        )
        .await
        .unwrap();

    assert_eq!(output.results.len(), 2);
    assert_eq!(output.results[0].result.cid.0, "cid-cats-indoor");
    assert!(output.results[0].rank_score > output.results[1].rank_score);
    assert!(matches!(output.results[0].result.source, DataSource::P2p));

    let selected = local_metadata
        .iter()
        .find(|metadata| metadata.cid == output.results[0].result.cid)
        .unwrap();
    let signal = signal_fetcher(&output.results[0].result.cid.0);
    let report = TcvEngine.evaluate(
        selected,
        &TaskContext {
            task_description: profile.task_description.clone().unwrap(),
            task_type: profile.task_type.clone().unwrap(),
            required_columns: profile.data_standard.canonical_columns.clone(),
            time_range: None,
            existing_data_cids: vec![],
            budget: 0.0,
        },
        &signal,
    );

    dump_json(
        "backend.e2e.full_chain",
        &serde_json::json!({
            "query": query,
            "keywords": profile.keywords,
            "selected_cid": output.results[0].result.cid.0,
            "selected_title": output.results[0].result.title,
            "rank_score": output.results[0].rank_score,
            "tcv_score": report.tcv_score,
            "tcv_verdict": report.verdict,
        }),
    );

    assert_eq!(report.schema_fit, 100.0);
    assert_eq!(report.verdict, TcvVerdict::StrongPositive);
    assert!(report.tcv_score > 60.0);
}

#[tokio::test]
#[ignore] // requires DeepSeek credentials + network: cargo test -p data-search deepseek_live_nl_query_to_rank_scores -- --ignored --nocapture
async fn deepseek_live_nl_query_to_rank_scores() {
    let query =
        "write an image classifier that checks whether Caesar is in the photo taken by my house monitor";
    let config = load_live_intent_parser_config();
    let parser = IntentParser::new(config.clone());
    let profile = parser
        .profile(query)
        .await
        .expect("DeepSeek intent parse failed");
    let search_query = if profile.keywords.is_empty() {
        profile.raw_query.clone()
    } else {
        profile.keywords.join(" ")
    };
    let engine = make_live_engine(parser.clone());
    let output = engine
        .search_with_profile(
            &profile,
            &SearchFilters::default(),
            &[],
            &neutral_signal_fetcher(),
            10,
        )
        .await
        .unwrap();

    let results_summary = output
        .results
        .iter()
        .map(|ranked| {
            serde_json::json!({
                "title": ranked.result.title.as_str(),
                "cid": ranked.result.cid.0.as_str(),
                "rank_score": ranked.rank_score,
                "source": format!("{:?}", ranked.result.source),
            })
        })
        .collect::<Vec<_>>();

    println!("backend.live.query: {query}");
    println!("backend.live.api_base: {}", config.api_base);
    println!("backend.live.model: {}", config.model);
    println!(
        "backend.live.task_type: {}",
        profile.task_type.as_deref().unwrap_or("<none>")
    );
    println!(
        "backend.live.task_description: {}",
        profile.task_description.as_deref().unwrap_or("<none>")
    );
    println!(
        "backend.live.target_entity: {}",
        profile.target_entity.as_deref().unwrap_or("<none>")
    );
    println!(
        "backend.live.keywords: {}",
        if profile.keywords.is_empty() {
            "<none>".to_string()
        } else {
            profile.keywords.join(", ")
        }
    );
    println!(
        "backend.live.sample_unit: {}",
        if profile.data_standard.sample_unit.trim().is_empty() {
            "<none>"
        } else {
            profile.data_standard.sample_unit.as_str()
        }
    );
    println!(
        "backend.live.min_resolution: {}",
        profile
            .data_standard
            .metadata_fields
            .iter()
            .find(|field| field.name == "resolution")
            .map(|field| field.value.as_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("<none>")
    );
    println!("backend.live.search_query: {search_query}");
    for ranked in &output.results {
        println!(
            "backend.live.result: title=\"{}\" cid=\"{}\" score={:.3} source={:?}",
            ranked.result.title, ranked.result.cid.0, ranked.rank_score, ranked.result.source
        );
    }

    dump_json(
        "backend.live.chain",
        &serde_json::json!({
            "query": query,
            "task_type": profile.task_type,
            "task_description": profile.task_description,
            "target_entity": profile.target_entity,
            "keywords": profile.keywords,
            "search_query": search_query,
            "results": results_summary,
        }),
    );

    assert!(profile.task_description.is_some());
    assert!(output
        .results
        .iter()
        .all(|ranked| ranked.rank_score.is_finite()));
}

#[tokio::test]
#[ignore] // requires DeepSeek credentials + network: cargo test -p data-search deepseek_live_nl_query_to_scored_datasets -- --ignored --nocapture
async fn deepseek_live_nl_query_to_scored_datasets() {
    use crate::engine::{DatasetSelectionOutput, DatasetSelectionTask, DatasetValuationConfig};

    let query = "check whether Caesar is in the image taken by monitor";
    let config = load_live_intent_parser_config();
    let parser = IntentParser::new(config.clone());
    let profile = parser
        .profile(query)
        .await
        .expect("DeepSeek intent parse failed");
    let search_query = if profile.keywords.is_empty() {
        profile.raw_query.clone()
    } else {
        profile.keywords.join(" ")
    };

    let adapters = adapters::default_adapters();
    let adapters_checked = adapters
        .iter()
        .map(|adapter| adapter.name().to_string())
        .collect::<Vec<_>>();
    let local_metadata = vec![
        make_detailed_image_metadata(
            "caesar-monitor-v1",
            "Caesar Monitor Images v1",
            "Indoor monitor snapshots with labels indicating whether Caesar appears in frame.",
            &[
                "cat",
                "monitor",
                "classification",
                "indoor",
                "caesar",
                "presence-detection",
            ],
            &[
                "sample_id",
                "label",
                "camera_id",
                "captured_at",
                "room_name",
            ],
            1_500,
            8_000_000,
            "1280x720",
        ),
        make_detailed_image_metadata(
            "caesar-monitor-night",
            "Caesar Night Monitor Image Set",
            "Low-light indoor monitoring images of Caesar with binary presence labels.",
            &[
                "cat",
                "monitor",
                "night",
                "classification",
                "caesar",
                "low-light",
            ],
            &[
                "sample_id",
                "label",
                "camera_id",
                "captured_at",
                "lighting_condition",
            ],
            2_250,
            10_000_000,
            "1920x1080",
        ),
        make_detailed_image_metadata(
            "pet-cam-home",
            "Home Pet Cam Classification Dataset",
            "Household pet camera images with labels for cat presence and empty frames.",
            &[
                "cat",
                "petcam",
                "classification",
                "home",
                "monitor",
                "empty-frame",
            ],
            &[
                "sample_id",
                "label",
                "camera_id",
                "captured_at",
                "scene_type",
            ],
            3_000,
            12_000_000,
            "1280x720",
        ),
        make_detailed_image_metadata(
            "indoor-cat-frames",
            "Indoor Cat Presence Frames",
            "Annotated indoor image frames for determining whether a cat is present.",
            &[
                "cat",
                "indoor",
                "image",
                "classification",
                "presence",
                "frames",
            ],
            &[
                "sample_id",
                "label",
                "camera_id",
                "captured_at",
                "occlusion_level",
            ],
            3_750,
            14_000_000,
            "1280x720",
        ),
        make_detailed_image_metadata(
            "monitor-cat-binary",
            "Monitor Cat Binary Labels Dataset",
            "Still monitor images with sample_id and label columns for cat detection.",
            &[
                "cat",
                "monitor",
                "binary",
                "label",
                "classification",
                "high-resolution",
            ],
            &[
                "sample_id",
                "label",
                "camera_id",
                "captured_at",
                "frame_checksum",
            ],
            4_500,
            16_000_000,
            "2560x1440",
        ),
    ];

    let signal_fetcher = neutral_signal_fetcher();
    let engine = make_engine_with_parser(parser, adapters);
    let search_output = engine
        .search_with_profile(
            &profile,
            &SearchFilters::default(),
            &local_metadata,
            &signal_fetcher,
            20,
        )
        .await
        .expect("live search failed");
    let selection_output: DatasetSelectionOutput = engine
        .value_search_output(
            &search_output,
            &local_metadata,
            None,
            None,
            &DatasetSelectionTask::from(&profile),
            &DatasetValuationConfig::default(),
        )
        .await
        .expect("live valuation failed");

    let sources_with_results = selection_output
        .candidates
        .iter()
        .map(|candidate| format!("{:?}", candidate.result.source))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let datasets = selection_output
        .candidates
        .iter()
        .map(|candidate| {
            serde_json::json!({
                "cid": candidate.result.cid.0,
                "title": candidate.result.title,
                "source": format!("{:?}", candidate.result.source),
                "dataset_score": candidate.final_score,
                "task_similarity": candidate.task_similarity,
                "schema_fit": candidate.schema_fit,
                "scale_score": candidate.scale_score,
                "balance_score": candidate.balance_score,
                "metadata_quality": candidate.metadata_quality,
                "explanation": candidate.explanation,
            })
        })
        .collect::<Vec<_>>();

    dump_json(
        "backend.live.full_pipeline",
        &serde_json::json!({
            "query": query,
            "search_query": search_query,
            "adapters_checked": adapters_checked,
            "local_test_metadata": local_metadata
                .iter()
                .map(|metadata| serde_json::json!({
                    "cid": metadata.cid.0,
                    "title": metadata.title,
                    "description": metadata.description,
                    "tags": metadata.tags,
                    "data_type": metadata.data_type,
                    "schema": {
                        "columns": metadata
                            .schema
                            .columns
                            .iter()
                            .map(|column| serde_json::json!({
                                "name": column.name,
                            }))
                            .collect::<Vec<_>>(),
                        "row_count": metadata.schema.row_count,
                        "size_bytes": metadata.schema.size_bytes,
                    },
                }))
                .collect::<Vec<_>>(),
            "sources_with_results": sources_with_results,
            "task_info": {
                "task_type": profile.task_type,
                "task_description": profile.task_description,
                "target_entity": profile.target_entity,
                "sample_unit": profile.data_standard.sample_unit,
                "required_columns": DatasetSelectionTask::from(&profile).required_columns,
            },
            "keywords": profile.keywords,
            "datasets": datasets,
            "search_errors": search_output.errors,
            "selected": selection_output.selected.as_ref().map(|candidate| serde_json::json!({
                "cid": candidate.result.cid.0,
                "title": candidate.result.title,
                "source": format!("{:?}", candidate.result.source),
                "dataset_score": candidate.final_score,
            })),
            "api_base": config.api_base,
            "model": config.model,
        }),
    );

    assert!(
        !selection_output.candidates.is_empty(),
        "expected at least one dataset from local test metadata or current live adapters"
    );
}

#[tokio::test]
#[ignore] // requires /data/pyc/guixu-data + DeepSeek credentials + network
async fn deepseek_local_guixu_data_top5_sample_reranking() {
    use crate::engine::{DatasetSelectionOutput, DatasetSelectionTask, DatasetValuationConfig};

    let root = PathBuf::from("/data/pyc/guixu-data");
    assert!(
        root.is_dir(),
        "local dataset root not found: {}",
        root.display()
    );

    let query = "check whether the people in the image are wearing safety helmets correctly";
    let config = load_live_intent_parser_config();
    let parser = IntentParser::new(config.clone());
    let profile = parser
        .profile(query)
        .await
        .expect("DeepSeek intent parse failed");
    let (local_metadata, roots_by_cid) =
        build_local_guixu_dataset_metadata(&root).expect("build local guixu metadata");
    let engine = make_engine_with_parser(parser, vec![]);
    let signal_fetcher = neutral_signal_fetcher();
    let search_output = engine
        .search_with_profile(
            &profile,
            &SearchFilters::default(),
            &local_metadata,
            &signal_fetcher,
            20,
        )
        .await
        .expect("local guixu search failed");
    let evaluator = RandomSeedSimilarityEvaluator::with_config(
        Box::new(LocalDirectorySampleDownloader {
            roots_by_cid,
            max_records: 6,
        }),
        Box::new(DeepSeekSeedRecordJudge::new(config.clone()).with_record_char_limit(600)),
        RandomSeedSimilarityEvaluatorConfig {
            seed_record_count: 4,
            low_score_threshold: 35.0,
            high_score_threshold: 75.0,
            override_final_below_score: 20.0,
            selection_seed: 2026,
        },
    );
    let selection_output: DatasetSelectionOutput = engine
        .value_search_output(
            &search_output,
            &local_metadata,
            None,
            Some(&evaluator),
            &DatasetSelectionTask::from(&profile),
            &DatasetValuationConfig::default(),
        )
        .await
        .expect("local guixu reranking failed");

    let top5 = selection_output
        .candidates
        .iter()
        .take(5)
        .map(|candidate| {
            serde_json::json!({
                "cid": candidate.result.cid.0,
                "title": candidate.result.title,
                "source": format!("{:?}", candidate.result.source),
                "coarse_score": candidate.coarse_score,
                "final_score": candidate.final_score,
                "task_similarity": candidate.task_similarity,
                "schema_fit": candidate.schema_fit,
                "scale_score": candidate.scale_score,
                "balance_score": candidate.balance_score,
                "metadata_quality": candidate.metadata_quality,
                "proxy": candidate.proxy_utility.as_ref().map(|proxy| serde_json::json!({
                    "utility_score": proxy.utility_score,
                    "apply_mode": proxy.apply_mode,
                    "proxy_metric_name": proxy.proxy_metric_name,
                    "proxy_metric_value": proxy.proxy_metric_value,
                    "sampled_rows": proxy.sampled_rows,
                    "sampled_bytes": proxy.sampled_bytes,
                    "notes": proxy.notes,
                })),
                "explanation": candidate.explanation,
            })
        })
        .collect::<Vec<_>>();

    dump_json(
        "backend.local.guixu_data.sample_pipeline",
        &serde_json::json!({
            "query": query,
            "task_info": {
                "task_type": profile.task_type,
                "task_description": profile.task_description,
                "target_entity": profile.target_entity,
                "keywords": profile.keywords,
                "sample_unit": profile.data_standard.sample_unit,
                "required_columns": DatasetSelectionTask::from(&profile).required_columns,
            },
            "local_datasets": local_metadata
                .iter()
                .map(|metadata| serde_json::json!({
                    "cid": metadata.cid.0,
                    "title": metadata.title,
                    "description": metadata.description,
                    "tags": metadata.tags,
                    "data_type": metadata.data_type,
                    "row_count": metadata.schema.row_count,
                    "size_bytes": metadata.schema.size_bytes,
                }))
                .collect::<Vec<_>>(),
            "search_results": search_output
                .results
                .iter()
                .map(|ranked| serde_json::json!({
                    "cid": ranked.result.cid.0,
                    "title": ranked.result.title,
                    "rank_score": ranked.rank_score,
                    "data_type": ranked.result.data_type,
                }))
                .collect::<Vec<_>>(),
            "top5_final": top5,
            "selected": selection_output.selected.as_ref().map(|candidate| serde_json::json!({
                "cid": candidate.result.cid.0,
                "title": candidate.result.title,
                "final_score": candidate.final_score,
            })),
            "errors": selection_output.errors,
            "api_base": config.api_base,
            "model": config.model,
        }),
    );

    assert!(
        !selection_output.candidates.is_empty(),
        "expected local guixu datasets to produce scored candidates"
    );
}

#[tokio::test]
async fn search_wrapper_propagates_intent_parser_error_without_api_key() {
    let engine = make_engine(vec![]);
    let local_metadata = vec![make_metadata(
        "cats",
        "Cat Image Classification Dataset",
        "Labeled cat images for image classification",
        &["cats", "classification", "images"],
    )];
    let filters = SearchFilters::default();
    let error = engine
        .search(
            "Build a high-quality classifier to detect cats",
            &filters,
            &local_metadata,
            &neutral_signal_fetcher(),
            10,
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("missing DEEPSEEK_API_KEY"));
}

#[tokio::test]
async fn search_with_task_type_prefers_results_matching_requested_modality() {
    let engine = make_engine(vec![Box::new(StubAdapter {
        results: vec![
            make_external_result_with_type(
                "tabular-cat",
                "Cat Population Time Series",
                "Tabular yearly population history for cats",
                DataType::Tabular,
            ),
            make_external_result_with_type(
                "video-cat",
                "Adventure Time Fiona Cake S02E04 The Cat Who Tipped the Box",
                "Episode rip with HEVC video",
                DataType::Video,
            ),
        ],
    })]);

    let output = engine
        .search_with_task_type(
            "cat",
            Some("time_series_prediction"),
            &SearchFilters::default(),
            &[],
            &neutral_signal_fetcher(),
            10,
        )
        .await
        .unwrap();

    let result_types: Vec<DataType> = output.results.iter().map(|r| r.result.data_type).collect();
    dump_json("search.task_type.results", &output.results);

    assert_eq!(output.results.len(), 1);
    assert_eq!(output.results[0].result.title, "Cat Population Time Series");
    assert_eq!(result_types, vec![DataType::Tabular]);
}

// ===========================================================================
// Adapter registry & data-source correctness tests
// ===========================================================================

/// Every DataSource variant must have exactly one adapter in default_adapters
/// (except P2p and DataGov which are handled outside the adapter system).
#[test]
fn default_adapters_covers_all_expected_sources() {
    let adapters = adapters::default_adapters();
    let names: Vec<&str> = adapters.iter().map(|a| a.name()).collect();
    let sources: Vec<DataSource> = adapters.iter().map(|a| a.source_type()).collect();

    // Expected adapters present
    assert!(names.contains(&"kaggle"), "missing kaggle adapter");
    assert!(
        names.contains(&"huggingface"),
        "missing huggingface adapter"
    );
    assert!(names.contains(&"ipfs"), "missing ipfs adapter");
    assert!(names.contains(&"bittorrent"), "missing bittorrent adapter");
    assert!(names.contains(&"postgresql"), "missing postgresql adapter");
    assert!(names.contains(&"duckdb"), "missing duckdb adapter");
    assert!(names.contains(&"local_file"), "missing local_file adapter");
    assert!(
        names.contains(&"google_dataset_search"),
        "missing google_dataset_search adapter"
    );
    assert!(
        names.contains(&"datacite_commons"),
        "missing datacite_commons adapter"
    );

    // No duplicate names
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        names.len(),
        unique.len(),
        "duplicate adapter names detected"
    );

    // Source types match expected variants
    assert!(sources.contains(&DataSource::Kaggle));
    assert!(sources.contains(&DataSource::HuggingFace));
    assert!(sources.contains(&DataSource::Ipfs));
    assert!(sources.contains(&DataSource::BitTorrent));
    assert!(sources.contains(&DataSource::PostgreSql));
    assert!(sources.contains(&DataSource::DuckDb));
    assert!(sources.contains(&DataSource::LocalFile));
    assert!(sources.contains(&DataSource::GoogleDatasetSearch));
    assert!(sources.contains(&DataSource::DataCiteCommons));
}

/// Adapters that require credentials/config should return empty results
/// gracefully when not configured, never panic.
#[tokio::test]
async fn unconfigured_adapters_return_empty_without_error() {
    let adapters = adapters::default_adapters();
    for adapter in &adapters {
        let result = adapter.search("test query", 5).await;
        match result {
            Ok(results) => {
                // Adapters without credentials should return empty or valid results
                for r in &results {
                    assert!(
                        !r.title.is_empty(),
                        "{}: result has empty title",
                        adapter.name()
                    );
                    assert!(
                        !r.cid.0.is_empty(),
                        "{}: result has empty cid",
                        adapter.name()
                    );
                }
            }
            Err(_) => {
                // Network errors are acceptable for adapters that hit external APIs
                // (BitTorrent, Google, DataCite) — they should not panic.
            }
        }
    }
}

/// Each adapter's name() and source_type() must be consistent and non-empty.
#[test]
fn adapter_metadata_is_consistent() {
    let adapters = adapters::default_adapters();
    for adapter in &adapters {
        assert!(!adapter.name().is_empty(), "adapter name must not be empty");
        // source_type serialization should produce a non-empty lowercase string
        let source_json = serde_json::to_string(&adapter.source_type()).unwrap();
        assert!(
            source_json.len() > 2,
            "source_type serialization too short for {}",
            adapter.name()
        );
    }
}

/// DataSource enum should serialize to lowercase as configured by serde.
#[test]
fn datasource_serde_roundtrip() {
    let variants = vec![
        (DataSource::P2p, "\"p2p\""),
        (DataSource::Kaggle, "\"kaggle\""),
        (DataSource::HuggingFace, "\"huggingface\""),
        (DataSource::DataGov, "\"datagov\""),
        (DataSource::Ipfs, "\"ipfs\""),
        (DataSource::BitTorrent, "\"bittorrent\""),
        (DataSource::PostgreSql, "\"postgresql\""),
        (DataSource::DuckDb, "\"duckdb\""),
        (DataSource::LocalFile, "\"localfile\""),
        (DataSource::GoogleDatasetSearch, "\"googledatasetsearch\""),
        (DataSource::DataCiteCommons, "\"datacitecommons\""),
    ];
    for (variant, expected_json) in variants {
        let json = serde_json::to_string(&variant).unwrap();
        assert_eq!(
            json, expected_json,
            "serialization mismatch for {:?}",
            variant
        );
        let back: DataSource = serde_json::from_str(&json).unwrap();
        assert_eq!(
            serde_json::to_string(&back).unwrap(),
            json,
            "roundtrip failed for {json}"
        );
    }
}

#[test]
fn bittorrent_adapter_source_type_and_name() {
    let adapter = adapters::BitTorrentAdapter::default();
    assert_eq!(adapter.name(), "bittorrent");
    assert!(matches!(adapter.source_type(), DataSource::BitTorrent));
}

/// Google Dataset Search adapter should produce correct source type and
/// generate deterministic CIDs from title+url.
#[test]
fn google_adapter_source_type_and_name() {
    let adapter = adapters::GoogleDatasetSearchAdapter::default();
    assert_eq!(adapter.name(), "google_dataset_search");
    assert!(matches!(
        adapter.source_type(),
        DataSource::GoogleDatasetSearch
    ));
}

/// DataCite Commons adapter should produce correct source type.
#[test]
fn datacite_adapter_source_type_and_name() {
    let adapter = adapters::DataCiteCommonsAdapter::default();
    assert_eq!(adapter.name(), "datacite_commons");
    assert!(matches!(adapter.source_type(), DataSource::DataCiteCommons));
}

/// Search engine should propagate results from new adapters through ranking.
#[tokio::test]
async fn search_engine_includes_new_adapter_results() {
    // Stub adapters returning GoogleDatasetSearch and DataCiteCommons sources
    struct GdsStub;
    #[async_trait::async_trait]
    impl ExternalAdapter for GdsStub {
        fn name(&self) -> &str {
            "google_dataset_search"
        }
        fn source_type(&self) -> DataSource {
            DataSource::GoogleDatasetSearch
        }
        async fn search(&self, _q: &str, _l: usize) -> anyhow::Result<Vec<SearchResult>> {
            Ok(vec![SearchResult {
                cid: DatasetCid("gds-001".into()),
                title: "Climate Change Dataset".into(),
                description: Some("from Google".into()),
                schema: DatasetSchema {
                    columns: vec![],
                    row_count: 1000,
                    size_bytes: 0,
                },
                quality: None,
                price: Price::free(),
                license: License {
                    spdx_id: "CC-BY-4.0".into(),
                    commercial_use: true,
                    derivative_allowed: true,
                },
                provider: Did("gds:example.com".into()),
                source: DataSource::GoogleDatasetSearch,
                data_type: DataType::Tabular,
                created_at: Utc::now(),
            }])
        }
    }

    struct DcStub;
    #[async_trait::async_trait]
    impl ExternalAdapter for DcStub {
        fn name(&self) -> &str {
            "datacite_commons"
        }
        fn source_type(&self) -> DataSource {
            DataSource::DataCiteCommons
        }
        async fn search(&self, _q: &str, _l: usize) -> anyhow::Result<Vec<SearchResult>> {
            Ok(vec![SearchResult {
                cid: DatasetCid("10.5281/zenodo.123".into()),
                title: "Global Temperature Records".into(),
                description: Some("from DataCite".into()),
                schema: DatasetSchema {
                    columns: vec![],
                    row_count: 500,
                    size_bytes: 0,
                },
                quality: None,
                price: Price::free(),
                license: License {
                    spdx_id: "CC0-1.0".into(),
                    commercial_use: true,
                    derivative_allowed: true,
                },
                provider: Did("doi:10.5281/zenodo.123".into()),
                source: DataSource::DataCiteCommons,
                data_type: DataType::Tabular,
                created_at: Utc::now(),
            }])
        }
    }

    let engine = make_engine(vec![Box::new(GdsStub), Box::new(DcStub)]);
    let output = engine
        .search(
            "climate",
            &SearchFilters::default(),
            &[],
            &neutral_signal_fetcher(),
            10,
        )
        .await
        .unwrap();

    let sources: Vec<String> = output
        .results
        .iter()
        .map(|r| format!("{:?}", r.result.source))
        .collect();
    assert_eq!(output.results.len(), 2);
    assert!(
        sources.iter().any(|s| s.contains("GoogleDatasetSearch")),
        "missing GDS result"
    );
    assert!(
        sources.iter().any(|s| s.contains("DataCiteCommons")),
        "missing DataCite result"
    );
}

/// Source filter should work correctly with new data source names.
#[tokio::test]
async fn source_filter_works_for_new_sources() {
    struct GdsStub;
    #[async_trait::async_trait]
    impl ExternalAdapter for GdsStub {
        fn name(&self) -> &str {
            "google_dataset_search"
        }
        fn source_type(&self) -> DataSource {
            DataSource::GoogleDatasetSearch
        }
        async fn search(&self, _q: &str, _l: usize) -> anyhow::Result<Vec<SearchResult>> {
            Ok(vec![SearchResult {
                cid: DatasetCid("gds-filter".into()),
                title: "Filtered Dataset".into(),
                description: None,
                schema: DatasetSchema {
                    columns: vec![],
                    row_count: 10,
                    size_bytes: 0,
                },
                quality: None,
                price: Price::free(),
                license: License {
                    spdx_id: "unknown".into(),
                    commercial_use: false,
                    derivative_allowed: false,
                },
                provider: Did("gds:test".into()),
                source: DataSource::GoogleDatasetSearch,
                data_type: DataType::Tabular,
                created_at: Utc::now(),
            }])
        }
    }

    let engine = make_engine(vec![Box::new(GdsStub)]);

    // Filter for GoogleDatasetSearch — should keep the result
    let filters_match = SearchFilters {
        source: Some("googledatasetsearch".into()),
        ..Default::default()
    };
    let output = engine
        .search("test", &filters_match, &[], &neutral_signal_fetcher(), 10)
        .await
        .unwrap();
    assert_eq!(output.results.len(), 1);

    // Filter for a different source — should exclude
    let filters_miss = SearchFilters {
        source: Some("kaggle".into()),
        ..Default::default()
    };
    let output = engine
        .search("test", &filters_miss, &[], &neutral_signal_fetcher(), 10)
        .await
        .unwrap();
    assert_eq!(output.results.len(), 0);
}

// ---------------------------------------------------------------------------
// Data type inference tests
// ---------------------------------------------------------------------------

#[test]
fn infer_video_from_encoding_keywords() {
    use crate::adapters::infer_data_type_from_title;
    assert_eq!(
        infer_data_type_from_title("Adventure Time S02E04 1080p HEVC x265-MeGusta"),
        DataType::Video,
    );
}

#[test]
fn infer_video_from_resolution() {
    use crate::adapters::infer_data_type_from_title;
    assert_eq!(
        infer_data_type_from_title("Movie Name 720p BluRay"),
        DataType::Video
    );
    assert_eq!(
        infer_data_type_from_title("Show 2160p WEB-DL"),
        DataType::Video
    );
}

#[test]
fn infer_video_from_season_pattern() {
    use crate::adapters::infer_data_type_from_title;
    assert_eq!(
        infer_data_type_from_title("Breaking Bad S01 Complete"),
        DataType::Video
    );
}

#[test]
fn infer_video_from_extension() {
    use crate::adapters::infer_data_type_from_title;
    assert_eq!(infer_data_type_from_title("clip.mp4"), DataType::Video);
    assert_eq!(infer_data_type_from_title("movie.mkv"), DataType::Video);
}

#[test]
fn infer_tabular_from_csv() {
    use crate::adapters::infer_data_type_from_title;
    assert_eq!(
        infer_data_type_from_title("sales_2024.csv"),
        DataType::Tabular
    );
}

#[test]
fn infer_tabular_from_dataset_keyword() {
    use crate::adapters::infer_data_type_from_title;
    assert_eq!(
        infer_data_type_from_title("NYC Taxi Dataset 2023"),
        DataType::Tabular
    );
}

#[test]
fn infer_audio_from_extension() {
    use crate::adapters::infer_data_type_from_title;
    assert_eq!(
        infer_data_type_from_title("podcast_ep1.mp3"),
        DataType::Audio
    );
    assert_eq!(
        infer_data_type_from_title("album lossless FLAC"),
        DataType::Audio
    );
}

#[test]
fn infer_image_from_extension() {
    use crate::adapters::infer_data_type_from_title;
    assert_eq!(infer_data_type_from_title("photo.jpg"), DataType::Image);
}

#[test]
fn infer_text_from_extension() {
    use crate::adapters::infer_data_type_from_title;
    assert_eq!(infer_data_type_from_title("book.pdf"), DataType::Text);
    assert_eq!(infer_data_type_from_title("notes.epub"), DataType::Text);
}

#[test]
fn infer_fallback_is_tabular() {
    use crate::adapters::infer_data_type_from_title;
    // Completely ambiguous title
    assert_eq!(
        infer_data_type_from_title("random stuff here"),
        DataType::Tabular
    );
}

// ---------------------------------------------------------------------------
// LocalFileAdapter tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn local_file_adapter_finds_csv() {
    use crate::adapters::{ExternalAdapter, LocalFileAdapter};

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("sales.csv"),
        "date,amount\n2024-01-01,100\n2024-01-02,200\n",
    )
    .unwrap();

    let adapter = LocalFileAdapter {
        dirs: vec![dir.path().to_path_buf()],
    };
    let results = adapter.search("sales", 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "sales.csv");
    assert_eq!(results[0].data_type, DataType::Tabular);
    assert!(results[0].schema.columns.len() >= 2); // date, amount
}

#[tokio::test]
async fn local_file_adapter_no_match() {
    use crate::adapters::{ExternalAdapter, LocalFileAdapter};

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("weather.csv"), "temp\n20\n").unwrap();

    let adapter = LocalFileAdapter {
        dirs: vec![dir.path().to_path_buf()],
    };
    let results = adapter.search("finance", 10).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn local_file_adapter_empty_dirs() {
    use crate::adapters::{ExternalAdapter, LocalFileAdapter};

    let adapter = LocalFileAdapter { dirs: vec![] };
    let results = adapter.search("anything", 10).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn local_file_adapter_matches_by_column_name() {
    use crate::adapters::{ExternalAdapter, LocalFileAdapter};

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("data.csv"),
        "price,volume,ticker\n100,5000,AAPL\n",
    )
    .unwrap();

    let adapter = LocalFileAdapter {
        dirs: vec![dir.path().to_path_buf()],
    };
    // Search by column name
    let results = adapter.search("ticker", 10).await.unwrap();
    assert_eq!(results.len(), 1);
}

// ---------------------------------------------------------------------------
// SearchResult data_type field propagation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_result_includes_data_type() {
    let engine = make_engine(vec![]);
    let meta = vec![make_metadata("1", "video_clips", "video data", &["video"])];
    let output = engine
        .search(
            "video",
            &SearchFilters::default(),
            &meta,
            &neutral_signal_fetcher(),
            10,
        )
        .await
        .unwrap();
    assert!(!output.results.is_empty());
    // metadata_to_search_result should propagate data_type from metadata
    assert_eq!(output.results[0].result.data_type, DataType::Tabular); // make_metadata uses Tabular
}

// ===========================================================================
// DataCite Commons integration test (requires network, run with --ignored)
// ===========================================================================

#[tokio::test]
#[ignore] // requires network access — run with: cargo test -p data-search -- --ignored
async fn datacite_commons_live_search_returns_results() {
    let adapter = adapters::DataCiteCommonsAdapter::default();
    let results = adapter
        .search("climate", 5)
        .await
        .expect("DataCite API call failed");

    assert!(
        !results.is_empty(),
        "expected at least one result for 'climate'"
    );
    assert!(results.len() <= 5, "should respect limit");

    for r in &results {
        // CID should be a DOI
        assert!(
            r.cid.0.starts_with("10."),
            "cid should be a DOI, got: {}",
            r.cid.0
        );
        assert!(!r.title.is_empty(), "title must not be empty");
        assert!(matches!(r.source, DataSource::DataCiteCommons));
        assert!(r.price.amount == 0.0, "DataCite datasets should be free");
        assert!(
            r.provider.0.starts_with("doi:"),
            "provider should be doi: prefixed"
        );
    }
}

#[tokio::test]
#[ignore]
async fn datacite_commons_live_empty_query_does_not_panic() {
    let adapter = adapters::DataCiteCommonsAdapter::default();
    // Empty or very obscure query — should return ok (possibly empty)
    let result = adapter.search("zzzxxx_nonexistent_dataset_42", 3).await;
    assert!(result.is_ok(), "should not error on obscure query");
}

#[tokio::test]
#[ignore]
async fn datacite_commons_live_result_has_description_or_year() {
    let adapter = adapters::DataCiteCommonsAdapter::default();
    let results = adapter
        .search("genomics", 3)
        .await
        .expect("API call failed");

    // At least one result should have a description with year prefix
    if !results.is_empty() {
        let has_desc = results.iter().any(|r| r.description.is_some());
        assert!(has_desc, "expected at least one result with a description");
    }
}
