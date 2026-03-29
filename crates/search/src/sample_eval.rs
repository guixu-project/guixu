use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use data_core::metadata::DatasetMetadata;
use data_core::types::{DataSource, SearchResult};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::engine::{
    DatasetSelectionTask, ProxyUtilityApplyMode, ProxyUtilityReport, SampleEvaluator, SamplePlan,
};
use crate::intent::IntentParserConfig;

/// Lightweight representation of a downloaded sample item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleRecord {
    pub id: String,
    pub content: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Small sample downloaded from a candidate dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadedSample {
    #[serde(default)]
    pub records: Vec<SampleRecord>,
    pub sampled_rows: u64,
    pub sampled_bytes: u64,
    pub summary: Option<String>,
}

/// Big-model plan for what evidence a useful sample should contain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleRequirements {
    pub summary: String,
    #[serde(default)]
    pub required_signals: Vec<String>,
    #[serde(default)]
    pub preferred_labels: Vec<String>,
    #[serde(default)]
    pub disqualifying_signals: Vec<String>,
}

/// Per-record pseudo labels produced by a local proxy scorer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyLabel {
    pub record_id: String,
    pub label: String,
    pub confidence: f64,
}

/// Local proxy screening before expensive LLM scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyScreeningReport {
    pub similarity_score: f64,
    #[serde(default)]
    pub proxy_labels: Vec<ProxyLabel>,
    #[serde(default)]
    pub matched_signals: Vec<String>,
    #[serde(default)]
    pub missing_signals: Vec<String>,
    pub rationale: String,
}

/// Expensive LLM judgement for promising samples.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleJudgeReport {
    pub utility_score: f64,
    pub rationale: String,
}

/// Per-record DeepSeek score for a randomly chosen seed subset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedRecordScore {
    pub record_id: String,
    pub utility_score: f64,
    pub rationale: String,
}

/// LLM output for the randomly chosen seed records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedRecordJudgeReport {
    pub summary: String,
    #[serde(default)]
    pub scored_records: Vec<SeedRecordScore>,
}

/// Download a small sample for candidate re-ranking.
#[async_trait::async_trait]
pub trait SampleDownloader: Send + Sync {
    async fn download_sample(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        plan: &SamplePlan,
    ) -> Result<Option<DownloadedSample>>;
}

/// Default placeholder until dataset-specific sample download is implemented.
pub struct NoopSampleDownloader;

#[async_trait::async_trait]
impl SampleDownloader for NoopSampleDownloader {
    async fn download_sample(
        &self,
        _result: &SearchResult,
        _metadata: &DatasetMetadata,
        _plan: &SamplePlan,
    ) -> Result<Option<DownloadedSample>> {
        Ok(None)
    }
}

/// Try multiple sample downloaders in order until one returns a sample.
pub struct ChainedSampleDownloader {
    downloaders: Vec<Box<dyn SampleDownloader>>,
}

impl ChainedSampleDownloader {
    pub fn new(downloaders: Vec<Box<dyn SampleDownloader>>) -> Self {
        Self { downloaders }
    }
}

#[async_trait::async_trait]
impl SampleDownloader for ChainedSampleDownloader {
    async fn download_sample(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        plan: &SamplePlan,
    ) -> Result<Option<DownloadedSample>> {
        for downloader in &self.downloaders {
            if let Some(sample) = downloader.download_sample(result, metadata, plan).await? {
                return Ok(Some(sample));
            }
        }
        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct GuixuHubSampleDownloader {
    client: Client,
    download_api_url: String,
    extract_root: PathBuf,
    max_records: usize,
    max_text_chars: usize,
}

impl Default for GuixuHubSampleDownloader {
    fn default() -> Self {
        Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(3))
                .timeout(Duration::from_secs(20))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| Client::new()),
            download_api_url: std::env::var("GUIXU_HUB_DOWNLOAD_API_URL")
                .unwrap_or_else(|_| "https://www.guixu.org/api/download".into()),
            extract_root: std::env::temp_dir().join("guixu-sample-cache"),
            max_records: 24,
            max_text_chars: 2_000,
        }
    }
}

impl GuixuHubSampleDownloader {
    pub fn new(download_api_url: impl Into<String>) -> Self {
        Self {
            download_api_url: download_api_url.into(),
            ..Self::default()
        }
    }

    pub fn with_extract_root(mut self, extract_root: impl Into<PathBuf>) -> Self {
        self.extract_root = extract_root.into();
        self
    }

    pub fn with_record_limit(mut self, max_records: usize) -> Self {
        self.max_records = max_records.max(1);
        self
    }

    pub fn with_text_char_limit(mut self, max_text_chars: usize) -> Self {
        self.max_text_chars = max_text_chars.max(128);
        self
    }
}

#[derive(Debug, Deserialize)]
struct GuixuHubSampleDownloadResponse {
    #[serde(rename = "downloadUrl")]
    download_url: String,
}

#[async_trait::async_trait]
impl SampleDownloader for GuixuHubSampleDownloader {
    async fn download_sample(
        &self,
        result: &SearchResult,
        _metadata: &DatasetMetadata,
        plan: &SamplePlan,
    ) -> Result<Option<DownloadedSample>> {
        if result.source != DataSource::GuixuHub {
            return Ok(None);
        }

        let Some(listing_id) = guixu_hub_listing_id(result) else {
            return Ok(None);
        };
        let sample_url = self.fetch_sample_url(&listing_id).await?;
        if sample_url.trim().is_empty() {
            return Ok(None);
        }

        let response = self
            .client
            .get(&sample_url)
            .send()
            .await
            .with_context(|| format!("download Guixu Hub sample archive for {listing_id}"))?
            .error_for_status()
            .with_context(|| format!("Guixu Hub sample archive returned error for {listing_id}"))?;
        let final_url = response.url().to_string();
        let archive_bytes = response
            .bytes()
            .await
            .with_context(|| format!("read Guixu Hub sample archive for {listing_id}"))?;
        if archive_bytes.is_empty() {
            return Ok(None);
        }

        let max_records = self
            .max_records
            .min(plan.max_rows.max(1) as usize)
            .max(1);
        let extracted_root = build_guixu_sample_extract_root(&self.extract_root, &listing_id);

        let sample = parse_guixu_hub_sample_archive(
            archive_bytes.as_ref(),
            &listing_id,
            &final_url,
            &extracted_root,
            max_records,
            self.max_text_chars,
        )
        .or_else(|_| {
            parse_guixu_hub_sample_blob(
                archive_bytes.as_ref(),
                &listing_id,
                &final_url,
                &extracted_root,
                self.max_text_chars,
            )
        })?;

        if sample.records.is_empty() {
            Ok(None)
        } else {
            Ok(Some(sample))
        }
    }
}

/// Ask a frontier model what kind of sample evidence is needed for the task.
#[async_trait::async_trait]
pub trait SampleRequirementsPlanner: Send + Sync {
    async fn plan_requirements(&self, task: &DatasetSelectionTask) -> Result<SampleRequirements>;
}

/// Local proxy scorer that produces pseudo labels and a similarity score.
#[async_trait::async_trait]
pub trait ProxySampleScorer: Send + Sync {
    async fn screen_sample(
        &self,
        sample: &DownloadedSample,
        requirements: &SampleRequirements,
        task: &DatasetSelectionTask,
    ) -> Result<ProxyScreeningReport>;
}

/// Escalate promising samples to a stronger model for final judgement.
#[async_trait::async_trait]
pub trait LlmSampleJudge: Send + Sync {
    async fn judge_sample(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        task: &DatasetSelectionTask,
        requirements: &SampleRequirements,
        sample: &DownloadedSample,
        screening: &ProxyScreeningReport,
    ) -> Result<SampleJudgeReport>;
}

/// Judge a small random subset of sample records with a stronger model.
#[async_trait::async_trait]
pub trait SeedRecordJudge: Send + Sync {
    async fn judge_records(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        task: &DatasetSelectionTask,
        sample: &DownloadedSample,
        records: &[SampleRecord],
    ) -> Result<SeedRecordJudgeReport>;
}

/// Runtime knobs for the random-seed + low-score-similarity evaluator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomSeedSimilarityEvaluatorConfig {
    pub seed_record_count: usize,
    pub low_score_threshold: f64,
    pub high_score_threshold: f64,
    pub override_final_below_score: f64,
    pub selection_seed: u64,
}

impl Default for RandomSeedSimilarityEvaluatorConfig {
    fn default() -> Self {
        Self {
            seed_record_count: 4,
            low_score_threshold: 35.0,
            high_score_threshold: 75.0,
            override_final_below_score: 20.0,
            selection_seed: 2026,
        }
    }
}

/// Runtime knobs for the staged sample evaluation pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedSampleEvaluatorConfig {
    pub proxy_similarity_threshold: f64,
    pub rejected_similarity_max_score: f64,
}

impl Default for StagedSampleEvaluatorConfig {
    fn default() -> Self {
        Self {
            proxy_similarity_threshold: 35.0,
            rejected_similarity_max_score: 15.0,
        }
    }
}

/// Two-stage sample evaluator:
/// 1. Big model plans what a good sample should look like.
/// 2. Local proxy scores sample similarity and emits pseudo labels.
/// 3. Low-similarity samples get a low score immediately.
/// 4. High-similarity samples are escalated to a bigger model for judgement.
pub struct StagedSampleEvaluator {
    downloader: Box<dyn SampleDownloader>,
    planner: Box<dyn SampleRequirementsPlanner>,
    proxy: Box<dyn ProxySampleScorer>,
    judge: Box<dyn LlmSampleJudge>,
    config: StagedSampleEvaluatorConfig,
    requirement_cache: Mutex<HashMap<String, SampleRequirements>>,
}

impl StagedSampleEvaluator {
    pub fn new(
        downloader: Box<dyn SampleDownloader>,
        planner: Box<dyn SampleRequirementsPlanner>,
        proxy: Box<dyn ProxySampleScorer>,
        judge: Box<dyn LlmSampleJudge>,
    ) -> Self {
        Self::with_config(
            downloader,
            planner,
            proxy,
            judge,
            StagedSampleEvaluatorConfig::default(),
        )
    }

    pub fn with_config(
        downloader: Box<dyn SampleDownloader>,
        planner: Box<dyn SampleRequirementsPlanner>,
        proxy: Box<dyn ProxySampleScorer>,
        judge: Box<dyn LlmSampleJudge>,
        config: StagedSampleEvaluatorConfig,
    ) -> Self {
        Self {
            downloader,
            planner,
            proxy,
            judge,
            config,
            requirement_cache: Mutex::new(HashMap::new()),
        }
    }

    async fn requirements_for_task(
        &self,
        task: &DatasetSelectionTask,
    ) -> Result<SampleRequirements> {
        let cache_key = format!(
            "{}|{}|{}|{}",
            task.task_type,
            task.task_description,
            task.target_entity.clone().unwrap_or_default(),
            task.required_columns.join(",")
        );
        if let Some(cached) = self
            .requirement_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&cache_key)
            .cloned()
        {
            return Ok(cached);
        }

        let planned = self.planner.plan_requirements(task).await?;
        self.requirement_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(cache_key, planned.clone());
        Ok(planned)
    }
}

#[async_trait::async_trait]
impl SampleEvaluator for StagedSampleEvaluator {
    async fn evaluate_sample(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        task: &DatasetSelectionTask,
        plan: &SamplePlan,
    ) -> Result<Option<ProxyUtilityReport>> {
        let requirements = self.requirements_for_task(task).await?;
        let Some(sample) = self
            .downloader
            .download_sample(result, metadata, plan)
            .await?
        else {
            return Ok(None);
        };

        if sample.records.is_empty() {
            return Ok(None);
        }

        let screening = self
            .proxy
            .screen_sample(&sample, &requirements, task)
            .await?;
        if screening.similarity_score < self.config.proxy_similarity_threshold {
            let utility_score = ((screening.similarity_score / 100.0)
                * self.config.rejected_similarity_max_score)
                .clamp(0.0, self.config.rejected_similarity_max_score);
            return Ok(Some(ProxyUtilityReport {
                utility_score,
                apply_mode: ProxyUtilityApplyMode::OverrideFinal,
                proxy_metric_name: "proxy_requirement_similarity".into(),
                proxy_metric_value: screening.similarity_score,
                sampled_rows: sample.sampled_rows,
                sampled_bytes: sample.sampled_bytes,
                notes: Some(format!(
                    "screened out before llm review: {}; proxy_labels={}",
                    screening.rationale,
                    summarize_proxy_labels(&screening.proxy_labels)
                )),
            }));
        }

        let judgement = self
            .judge
            .judge_sample(result, metadata, task, &requirements, &sample, &screening)
            .await?;

        Ok(Some(ProxyUtilityReport {
            utility_score: judgement.utility_score.clamp(0.0, 100.0),
            apply_mode: ProxyUtilityApplyMode::Blend,
            proxy_metric_name: "proxy_requirement_similarity".into(),
            proxy_metric_value: screening.similarity_score,
            sampled_rows: sample.sampled_rows,
            sampled_bytes: sample.sampled_bytes,
            notes: Some(format!(
                "{}; requirements={}; proxy_labels={}",
                judgement.rationale,
                requirements.summary,
                summarize_proxy_labels(&screening.proxy_labels)
            )),
        }))
    }
}

/// Sample evaluator for the flow:
/// 1. Download a small sample from the top-ranked dataset.
/// 2. Randomly pick a few records and ask DeepSeek to score them.
/// 3. Split those records into high-score / low-score anchors.
/// 4. Score the remaining records locally by their cosine similarity to low-score anchors.
/// 5. Average the seed and propagated scores to estimate dataset utility.
pub struct RandomSeedSimilarityEvaluator {
    downloader: Box<dyn SampleDownloader>,
    judge: Box<dyn SeedRecordJudge>,
    config: RandomSeedSimilarityEvaluatorConfig,
}

impl RandomSeedSimilarityEvaluator {
    pub fn new(downloader: Box<dyn SampleDownloader>, judge: Box<dyn SeedRecordJudge>) -> Self {
        Self::with_config(
            downloader,
            judge,
            RandomSeedSimilarityEvaluatorConfig::default(),
        )
    }

    pub fn with_config(
        downloader: Box<dyn SampleDownloader>,
        judge: Box<dyn SeedRecordJudge>,
        config: RandomSeedSimilarityEvaluatorConfig,
    ) -> Self {
        Self {
            downloader,
            judge,
            config,
        }
    }
}

#[derive(Debug, Clone)]
struct ScoredSeedSampleRecord {
    record: SampleRecord,
    utility_score: f64,
    rationale: String,
}

#[async_trait::async_trait]
impl SampleEvaluator for RandomSeedSimilarityEvaluator {
    async fn evaluate_sample(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        task: &DatasetSelectionTask,
        plan: &SamplePlan,
    ) -> Result<Option<ProxyUtilityReport>> {
        let Some(sample) = self
            .downloader
            .download_sample(result, metadata, plan)
            .await?
        else {
            return Ok(None);
        };
        if sample.records.is_empty() {
            return Ok(None);
        }

        let seed_indices =
            select_seed_indices(result, task, &sample.records, &self.config).into_iter().collect::<HashSet<_>>();
        if seed_indices.is_empty() {
            return Ok(None);
        }

        let seed_records = sample
            .records
            .iter()
            .enumerate()
            .filter_map(|(index, record)| seed_indices.contains(&index).then_some(record.clone()))
            .collect::<Vec<_>>();
        let judged = self
            .judge
            .judge_records(result, metadata, task, &sample, &seed_records)
            .await?;
        let scored_seed_records = resolve_scored_seed_records(&seed_records, &judged)?;
        if scored_seed_records.is_empty() {
            return Ok(None);
        }

        let mut low_score_anchors = scored_seed_records
            .iter()
            .filter(|record| record.utility_score <= self.config.low_score_threshold)
            .cloned()
            .collect::<Vec<_>>();
        if low_score_anchors.is_empty() {
            if let Some(lowest) = scored_seed_records.iter().min_by(|left, right| {
                left.utility_score
                    .partial_cmp(&right.utility_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                low_score_anchors.push(lowest.clone());
            }
        }
        let high_score_anchors = scored_seed_records
            .iter()
            .filter(|record| record.utility_score >= self.config.high_score_threshold)
            .cloned()
            .collect::<Vec<_>>();

        let seed_ids = scored_seed_records
            .iter()
            .map(|record| record.record.id.clone())
            .collect::<HashSet<_>>();
        let mut all_scores = scored_seed_records
            .iter()
            .map(|record| record.utility_score.clamp(0.0, 100.0))
            .collect::<Vec<_>>();
        let mut propagated_scores = Vec::new();
        let mut low_anchor_similarities = Vec::new();

        for record in &sample.records {
            if seed_ids.contains(&record.id) {
                continue;
            }
            let low_similarity = average_similarity_to_low_score_anchors(record, &low_score_anchors);
            let inferred_score = (100.0 - low_similarity).clamp(0.0, 100.0);
            all_scores.push(inferred_score);
            propagated_scores.push(inferred_score);
            low_anchor_similarities.push(low_similarity);
        }

        let utility_score = average_or_zero(&all_scores).clamp(0.0, 100.0);
        let apply_mode = if utility_score <= self.config.override_final_below_score {
            ProxyUtilityApplyMode::OverrideFinal
        } else {
            ProxyUtilityApplyMode::Blend
        };
        let avg_low_anchor_similarity = average_or_zero(&low_anchor_similarities);
        let seed_avg = average_or_zero(
            &scored_seed_records
                .iter()
                .map(|record| record.utility_score)
                .collect::<Vec<_>>(),
        );
        let propagated_avg = average_or_zero(&propagated_scores);

        Ok(Some(ProxyUtilityReport {
            utility_score,
            apply_mode,
            proxy_metric_name: "avg_low_score_anchor_similarity".into(),
            proxy_metric_value: avg_low_anchor_similarity,
            sampled_rows: sample.sampled_rows,
            sampled_bytes: sample.sampled_bytes,
            notes: Some(format!(
                "seed_records={}, high_score_anchors={}, low_score_anchors={}, seed_avg={seed_avg:.1}, propagated_avg={propagated_avg:.1}, summary={}, low_anchor_ids={}, high_anchor_ids={}, seed_rationales={}",
                scored_seed_records.len(),
                high_score_anchors.len(),
                low_score_anchors.len(),
                judged.summary,
                summarize_scored_seed_ids(&low_score_anchors),
                summarize_scored_seed_ids(&high_score_anchors),
                summarize_scored_seed_rationales(&scored_seed_records),
            )),
        }))
    }
}

fn summarize_proxy_labels(labels: &[ProxyLabel]) -> String {
    labels
        .iter()
        .take(3)
        .map(|label| format!("{}:{:.0}", label.label, label.confidence))
        .collect::<Vec<_>>()
        .join(", ")
}

fn summarize_scored_seed_ids(records: &[ScoredSeedSampleRecord]) -> String {
    records
        .iter()
        .take(3)
        .map(|record| format!("{}:{:.0}", record.record.id.as_str(), record.utility_score))
        .collect::<Vec<_>>()
        .join(", ")
}

fn summarize_scored_seed_rationales(records: &[ScoredSeedSampleRecord]) -> String {
    records
        .iter()
        .take(3)
        .map(|record| {
            format!(
                "{}:{}",
                record.record.id.as_str(),
                record.rationale.as_str()
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn average_or_zero(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn select_seed_indices(
    result: &SearchResult,
    task: &DatasetSelectionTask,
    records: &[SampleRecord],
    config: &RandomSeedSimilarityEvaluatorConfig,
) -> Vec<usize> {
    if records.is_empty() {
        return vec![];
    }

    let seed_count = config.seed_record_count.max(1).min(records.len());
    let mut indices = (0..records.len()).collect::<Vec<_>>();
    if seed_count >= records.len() || config.selection_seed == 0 {
        indices.truncate(seed_count);
        return indices;
    }

    let mut hasher = DefaultHasher::new();
    config.selection_seed.hash(&mut hasher);
    result.cid.0.hash(&mut hasher);
    task.task_description.hash(&mut hasher);
    let combined_seed = hasher.finish();
    let mut rng = StdRng::seed_from_u64(combined_seed);
    indices.shuffle(&mut rng);
    indices.truncate(seed_count);
    indices.sort_unstable();
    indices
}

fn resolve_scored_seed_records(
    seed_records: &[SampleRecord],
    report: &SeedRecordJudgeReport,
) -> Result<Vec<ScoredSeedSampleRecord>> {
    let seed_records_by_id = seed_records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect::<HashMap<_, _>>();
    let mut resolved = Vec::new();

    for scored in &report.scored_records {
        let Some(record) = seed_records_by_id.get(scored.record_id.as_str()) else {
            continue;
        };
        resolved.push(ScoredSeedSampleRecord {
            record: (*record).clone(),
            utility_score: scored.utility_score.clamp(0.0, 100.0),
            rationale: scored.rationale.clone(),
        });
    }

    if resolved.is_empty() {
        return Err(anyhow!(
            "seed record judge did not return any valid record ids for the sampled subset"
        ));
    }

    Ok(resolved)
}

fn average_similarity_to_low_score_anchors(
    record: &SampleRecord,
    low_score_anchors: &[ScoredSeedSampleRecord],
) -> f64 {
    if low_score_anchors.is_empty() {
        return 0.0;
    }

    let record_text = build_record_text(record);
    low_score_anchors
        .iter()
        .map(|anchor| cosine_similarity(&record_text, &build_record_text(&anchor.record)) * 100.0)
        .sum::<f64>()
        / low_score_anchors.len() as f64
}

fn guixu_hub_listing_id(result: &SearchResult) -> Option<String> {
    result
        .cid
        .0
        .strip_prefix("guixu-hub:")
        .map(str::to_string)
        .or_else(|| {
            result
                .provider
                .0
                .strip_prefix("guixu:hub:")
                .map(str::to_string)
        })
}

impl GuixuHubSampleDownloader {
    async fn fetch_sample_url(&self, listing_id: &str) -> Result<String> {
        let response = self
            .client
            .get(&self.download_api_url)
            .query(&[("id", listing_id), ("type", "sample")])
            .send()
            .await
            .with_context(|| format!("request Guixu Hub sample URL for {listing_id}"))?
            .error_for_status()
            .with_context(|| format!("Guixu Hub sample URL request failed for {listing_id}"))?
            .json::<GuixuHubSampleDownloadResponse>()
            .await
            .with_context(|| format!("parse Guixu Hub sample URL response for {listing_id}"))?;
        Ok(response.download_url)
    }
}

fn build_guixu_sample_extract_root(base: &Path, listing_id: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    listing_id.hash(&mut hasher);
    base.join(format!("{}-{:016x}", sanitize_path_component(listing_id), hasher.finish()))
}

fn sanitize_path_component(input: &str) -> String {
    let sanitized = input
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "sample".into()
    } else {
        sanitized
    }
}

fn parse_guixu_hub_sample_archive(
    archive_bytes: &[u8],
    listing_id: &str,
    download_url: &str,
    extract_root: &Path,
    max_records: usize,
    max_text_chars: usize,
) -> Result<DownloadedSample> {
    let mut archive = zip::ZipArchive::new(Cursor::new(archive_bytes))
        .context("open Guixu Hub sample zip archive")?;
    let mut image_entries_by_stem = HashMap::new();
    let mut image_entry_names = Vec::new();
    let mut text_entry_names = Vec::new();

    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .with_context(|| format!("read archive entry #{index}"))?;
        if !file.is_file() {
            continue;
        }
        let name = file.name().to_string();
        let Some(file_name) = Path::new(&name).file_name().and_then(|value| value.to_str()) else {
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

    let mut records = Vec::new();
    let mut extracted_images = HashMap::<String, PathBuf>::new();

    for entry_name in text_entry_names.into_iter().take(max_records) {
        let bytes = {
            let mut file = archive
                .by_name(&entry_name)
                .with_context(|| format!("read archive entry {entry_name}"))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .with_context(|| format!("read bytes from archive entry {entry_name}"))?;
            bytes
        };
        let content = summarize_sample_entry(&entry_name, &bytes, max_text_chars);
        if content.trim().is_empty() {
            continue;
        }

        let mut metadata = serde_json::json!({
            "listing_id": listing_id,
            "archive_path": entry_name,
            "kind": archive_entry_kind(&entry_name),
            "source_download_url": download_url,
        });

        let entry_stem = stem_key(
            Path::new(&entry_name)
                .file_name()
                .and_then(|value| value.to_str())
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
            metadata["local_image_relative_path"] = serde_json::Value::String(
                image_path
                    .strip_prefix(extract_root)
                    .unwrap_or(&image_path)
                    .display()
                    .to_string(),
            );
            metadata["local_image_mime_type"] =
                serde_json::Value::String(guess_image_mime_type(&image_path).to_string());
        }

        records.push(SampleRecord {
            id: entry_name.clone(),
            content,
            metadata,
        });
    }

    if records.is_empty() {
        for image_entry_name in image_entry_names.into_iter().take(max_records) {
            let image_path = extract_archive_entry_image(
                &mut archive,
                &image_entry_name,
                extract_root,
                &mut extracted_images,
            )?;
            records.push(SampleRecord {
                id: image_entry_name.clone(),
                content: format!(
                    "image sample {} from Guixu Hub listing {}",
                    Path::new(&image_entry_name)
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or(image_entry_name.as_str()),
                    listing_id
                ),
                metadata: serde_json::json!({
                    "listing_id": listing_id,
                    "archive_path": image_entry_name,
                    "kind": "image",
                    "source_download_url": download_url,
                    "local_image_path": image_path.display().to_string(),
                    "local_image_relative_path": image_path
                        .strip_prefix(extract_root)
                        .unwrap_or(&image_path)
                        .display()
                        .to_string(),
                    "local_image_mime_type": guess_image_mime_type(&image_path),
                }),
            });
        }
    }

    Ok(DownloadedSample {
        sampled_rows: records.len() as u64,
        sampled_bytes: archive_bytes.len() as u64,
        summary: Some(format!(
            "downloaded Guixu Hub sample archive for listing {} with {} records",
            listing_id,
            records.len()
        )),
        records,
    })
}

fn parse_guixu_hub_sample_blob(
    blob_bytes: &[u8],
    listing_id: &str,
    download_url: &str,
    extract_root: &Path,
    max_text_chars: usize,
) -> Result<DownloadedSample> {
    let guessed_name = download_url
        .split('?')
        .next()
        .and_then(|value| value.rsplit('/').next())
        .filter(|value| !value.is_empty())
        .unwrap_or("sample.bin");
    let sample_record = if is_image_path(guessed_name) {
        std::fs::create_dir_all(extract_root)
            .with_context(|| format!("create sample extract root {}", extract_root.display()))?;
        let image_path = extract_root.join(
            Path::new(guessed_name)
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("sample.bin")),
        );
        std::fs::write(&image_path, blob_bytes)
            .with_context(|| format!("write sample image {}", image_path.display()))?;
        SampleRecord {
            id: guessed_name.to_string(),
            content: format!("image sample {} from Guixu Hub listing {}", guessed_name, listing_id),
            metadata: serde_json::json!({
                "listing_id": listing_id,
                "kind": "image",
                "source_download_url": download_url,
                "local_image_path": image_path.display().to_string(),
                "local_image_relative_path": image_path
                    .strip_prefix(extract_root)
                    .unwrap_or(&image_path)
                    .display()
                    .to_string(),
                "local_image_mime_type": guess_image_mime_type(&image_path),
            }),
        }
    } else {
        SampleRecord {
            id: guessed_name.to_string(),
            content: summarize_sample_entry(guessed_name, blob_bytes, max_text_chars),
            metadata: serde_json::json!({
                "listing_id": listing_id,
                "kind": archive_entry_kind(guessed_name),
                "source_download_url": download_url,
            }),
        }
    };

    Ok(DownloadedSample {
        sampled_rows: 1,
        sampled_bytes: blob_bytes.len() as u64,
        summary: Some(format!("downloaded direct Guixu Hub sample blob for listing {listing_id}")),
        records: vec![sample_record],
    })
}

fn extract_archive_entry_image(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    entry_name: &str,
    extract_root: &Path,
    extracted_images: &mut HashMap<String, PathBuf>,
) -> Result<PathBuf> {
    if let Some(existing) = extracted_images.get(entry_name) {
        return Ok(existing.clone());
    }

    let mut file = archive
        .by_name(entry_name)
        .with_context(|| format!("read archive image entry {entry_name}"))?;
    let file_name = Path::new(entry_name)
        .file_name()
        .map(|value| value.to_owned())
        .unwrap_or_else(|| std::ffi::OsString::from("sample-image.bin"));
    std::fs::create_dir_all(extract_root)
        .with_context(|| format!("create sample extract root {}", extract_root.display()))?;
    let output_path = extract_root.join(file_name);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("read image bytes from archive entry {entry_name}"))?;
    std::fs::write(&output_path, &bytes)
        .with_context(|| format!("write extracted image {}", output_path.display()))?;
    extracted_images.insert(entry_name.to_string(), output_path.clone());
    Ok(output_path)
}

fn is_image_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "bmp" | "webp" | "gif")
    )
}

fn is_text_sample_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("xml" | "txt" | "json" | "jsonl" | "csv" | "tsv" | "md")
    )
}

fn stem_key(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn archive_entry_kind(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
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
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
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
        "json" | "jsonl" | "csv" | "tsv" | "txt" => text.lines().take(8).collect::<Vec<_>>().join(" "),
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

fn guess_image_mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("gif") => "image/gif",
        _ => "image/jpeg",
    }
}

/// Simple local proxy scorer based on token overlap / cosine similarity.
#[derive(Default)]
pub struct LocalHeuristicProxyScorer;

#[async_trait::async_trait]
impl ProxySampleScorer for LocalHeuristicProxyScorer {
    async fn screen_sample(
        &self,
        sample: &DownloadedSample,
        requirements: &SampleRequirements,
        task: &DatasetSelectionTask,
    ) -> Result<ProxyScreeningReport> {
        if sample.records.is_empty() {
            return Ok(ProxyScreeningReport {
                similarity_score: 0.0,
                proxy_labels: vec![],
                matched_signals: vec![],
                missing_signals: requirements.required_signals.clone(),
                rationale: "sample contains no records".into(),
            });
        }

        let requirement_text = build_requirement_text(requirements, task);
        let mut matched_signals = HashSet::new();
        let mut disqualifying_hits = 0_u64;
        let mut record_scores = Vec::with_capacity(sample.records.len());
        let mut proxy_labels = Vec::with_capacity(sample.records.len());
        let mut preferred_label_hits = 0_u64;

        for record in &sample.records {
            let record_text = build_record_text(record);
            let record_lower = record_text.to_lowercase();
            let similarity = cosine_similarity(&requirement_text, &record_text) * 100.0;
            let label = infer_proxy_label(&record_lower, similarity, requirements);

            for signal in &requirements.required_signals {
                if record_lower.contains(&signal.to_lowercase()) {
                    matched_signals.insert(signal.to_lowercase());
                }
            }
            if requirements
                .disqualifying_signals
                .iter()
                .any(|signal| record_lower.contains(&signal.to_lowercase()))
            {
                disqualifying_hits += 1;
            }
            if requirements
                .preferred_labels
                .iter()
                .any(|preferred| label.eq_ignore_ascii_case(preferred))
            {
                preferred_label_hits += 1;
            }

            record_scores.push(similarity);
            proxy_labels.push(ProxyLabel {
                record_id: record.id.clone(),
                label,
                confidence: similarity.clamp(0.0, 100.0),
            });
        }

        let avg_similarity = record_scores.iter().sum::<f64>() / record_scores.len() as f64;
        let signal_coverage = if requirements.required_signals.is_empty() {
            100.0
        } else {
            matched_signals.len() as f64 / requirements.required_signals.len() as f64 * 100.0
        };
        let preferred_label_rate = if requirements.preferred_labels.is_empty() {
            100.0
        } else {
            preferred_label_hits as f64 / sample.records.len() as f64 * 100.0
        };
        let disqualifying_penalty =
            (disqualifying_hits as f64 / sample.records.len() as f64) * 40.0;
        let similarity_score =
            (0.60 * avg_similarity + 0.25 * signal_coverage + 0.15 * preferred_label_rate
                - disqualifying_penalty)
                .clamp(0.0, 100.0);
        let missing_signals = requirements
            .required_signals
            .iter()
            .filter(|signal| !matched_signals.contains(&signal.to_lowercase()))
            .cloned()
            .collect::<Vec<_>>();
        let mut matched_signals = matched_signals.into_iter().collect::<Vec<_>>();
        matched_signals.sort();

        Ok(ProxyScreeningReport {
            similarity_score,
            proxy_labels,
            matched_signals: matched_signals.clone(),
            missing_signals: missing_signals.clone(),
            rationale: format!(
                "avg_similarity={avg_similarity:.1}, signal_coverage={signal_coverage:.1}, preferred_label_rate={preferred_label_rate:.1}, missing_signals={}",
                missing_signals.join("|")
            ),
        })
    }
}

fn build_requirement_text(
    requirements: &SampleRequirements,
    task: &DatasetSelectionTask,
) -> String {
    let mut parts = vec![
        requirements.summary.clone(),
        task.task_description.clone(),
        task.task_type.clone(),
    ];
    if !requirements.required_signals.is_empty() {
        parts.push(requirements.required_signals.join(" "));
    }
    if !requirements.preferred_labels.is_empty() {
        parts.push(requirements.preferred_labels.join(" "));
    }
    parts.join(" ")
}

fn build_record_text(record: &SampleRecord) -> String {
    let mut parts = vec![record.content.clone()];
    if !record.metadata.is_null() {
        parts.push(flatten_json_value(&record.metadata));
    }
    parts.join(" ")
}

fn flatten_json_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(v) => v.to_string(),
        serde_json::Value::Number(v) => v.to_string(),
        serde_json::Value::String(v) => v.clone(),
        serde_json::Value::Array(values) => values
            .iter()
            .map(flatten_json_value)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        serde_json::Value::Object(map) => map
            .iter()
            .flat_map(|(key, value)| [key.clone(), flatten_json_value(value)])
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn infer_proxy_label(
    record_lower: &str,
    similarity: f64,
    requirements: &SampleRequirements,
) -> String {
    if requirements
        .disqualifying_signals
        .iter()
        .any(|signal| record_lower.contains(&signal.to_lowercase()))
    {
        return "disqualifying".into();
    }

    if let Some(label) = requirements
        .preferred_labels
        .iter()
        .find(|label| record_lower.contains(&label.to_lowercase()))
    {
        return label.clone();
    }

    if requirements
        .required_signals
        .iter()
        .any(|signal| record_lower.contains(&signal.to_lowercase()))
        || similarity >= 60.0
    {
        "relevant".into()
    } else {
        "irrelevant".into()
    }
}

fn cosine_similarity(left: &str, right: &str) -> f64 {
    let left = term_frequency(tokenize(left));
    let right = term_frequency(tokenize(right));
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    let dot = left
        .iter()
        .map(|(token, weight)| weight * right.get(token).copied().unwrap_or(0.0))
        .sum::<f64>();
    let left_norm = left
        .values()
        .map(|weight| weight * weight)
        .sum::<f64>()
        .sqrt();
    let right_norm = right
        .values()
        .map(|weight| weight * weight)
        .sum::<f64>()
        .sqrt();

    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm * right_norm)
    }
}

fn term_frequency(tokens: Vec<String>) -> HashMap<String, f64> {
    let mut tf = HashMap::new();
    for token in tokens {
        *tf.entry(token).or_insert(0.0) += 1.0;
    }
    tf
}

fn tokenize(text: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "a", "an", "and", "as", "at", "be", "by", "data", "dataset", "for", "from", "if", "in",
        "into", "is", "of", "on", "or", "the", "to", "with",
    ];

    text.split(|character: char| !character.is_alphanumeric())
        .map(|token| token.to_lowercase())
        .filter(|token| token.len() > 1)
        .filter(|token| !STOPWORDS.contains(&token.as_str()))
        .collect()
}

/// LLM-backed sample requirement planner using the same DeepSeek setup as the intent parser.
pub struct DeepSeekSampleRequirementsPlanner {
    llm: DeepSeekJsonClient,
}

impl DeepSeekSampleRequirementsPlanner {
    pub fn from_env() -> Self {
        Self::new(IntentParserConfig::from_env())
    }

    pub fn new(config: IntentParserConfig) -> Self {
        Self {
            llm: DeepSeekJsonClient::new(config),
        }
    }
}

#[async_trait::async_trait]
impl SampleRequirementsPlanner for DeepSeekSampleRequirementsPlanner {
    async fn plan_requirements(&self, task: &DatasetSelectionTask) -> Result<SampleRequirements> {
        self.llm
            .chat_json(
                REQUIREMENTS_SYSTEM_PROMPT,
                build_requirements_user_prompt(task),
                256,
            )
            .await
    }
}

/// LLM-backed final scorer for high-similarity samples.
pub struct DeepSeekSampleJudge {
    llm: DeepSeekJsonClient,
    max_preview_records: usize,
}

impl DeepSeekSampleJudge {
    pub fn from_env() -> Self {
        Self::new(IntentParserConfig::from_env())
    }

    pub fn new(config: IntentParserConfig) -> Self {
        Self {
            llm: DeepSeekJsonClient::new(config),
            max_preview_records: 8,
        }
    }

    pub fn with_preview_limit(mut self, max_preview_records: usize) -> Self {
        self.max_preview_records = max_preview_records.max(1);
        self
    }
}

#[async_trait::async_trait]
impl LlmSampleJudge for DeepSeekSampleJudge {
    async fn judge_sample(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        task: &DatasetSelectionTask,
        requirements: &SampleRequirements,
        sample: &DownloadedSample,
        screening: &ProxyScreeningReport,
    ) -> Result<SampleJudgeReport> {
        self.llm
            .chat_json(
                SAMPLE_JUDGE_SYSTEM_PROMPT,
                build_sample_judge_user_prompt(
                    result,
                    metadata,
                    task,
                    requirements,
                    sample,
                    screening,
                    self.max_preview_records,
                )?,
                384,
            )
            .await
    }
}

/// Gemini image-eval backed judge for the random seed subset.
pub struct GeminiImageSeedRecordJudge {
    client: Client,
    api_url: String,
    max_record_chars: usize,
}

impl Default for GeminiImageSeedRecordJudge {
    fn default() -> Self {
        Self::from_env()
    }
}

impl GeminiImageSeedRecordJudge {
    pub fn from_env() -> Self {
        let api_url = std::env::var("GUIXU_GEMINI_EVAL_API_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:3000/api/search/gemini".into());
        Self::new(api_url)
    }

    pub fn new(api_url: impl Into<String>) -> Self {
        Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(3))
                .timeout(Duration::from_secs(30))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| Client::new()),
            api_url: normalize_gemini_eval_api_url(api_url.into()),
            max_record_chars: 800,
        }
    }

    pub fn with_record_char_limit(mut self, max_record_chars: usize) -> Self {
        self.max_record_chars = max_record_chars.max(64);
        self
    }

    async fn judge_record(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        task: &DatasetSelectionTask,
        record: &SampleRecord,
    ) -> Result<SeedRecordScore> {
        let image_path = sample_record_image_path(record)
            .ok_or_else(|| anyhow!("sample record {} missing local_image_path", record.id))?;
        let image_bytes = std::fs::read(&image_path)
            .with_context(|| format!("read sample image {}", image_path.display()))?;
        let mime_type = record
            .metadata
            .get("local_image_mime_type")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| guess_image_mime_type(&image_path).to_string());
        let request = GeminiImageEvalRequest {
            task: build_gemini_image_eval_task(
                result,
                metadata,
                task,
                record,
                self.max_record_chars,
            ),
            image: GeminiImageEvalImage {
                mime_type,
                data: base64::engine::general_purpose::STANDARD.encode(image_bytes),
            },
        };
        let response = self
            .client
            .post(&self.api_url)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("send Gemini image evaluation request for {}", record.id))?
            .error_for_status()
            .with_context(|| format!("Gemini image evaluation returned error for {}", record.id))?
            .json::<GeminiImageEvalResponse>()
            .await
            .with_context(|| format!("parse Gemini image evaluation response for {}", record.id))?;
        let utility_score = parse_gemini_numeric_result(&response).ok_or_else(|| {
            anyhow!(
                "Gemini image evaluation did not return a numeric score for {}: {}",
                record.id,
                if response.raw.trim().is_empty() {
                    response.result.to_string()
                } else {
                    response.raw.clone()
                }
            )
        })?;

        Ok(SeedRecordScore {
            record_id: record.id.clone(),
            utility_score: utility_score.clamp(0.0, 100.0),
            rationale: if response.raw.trim().is_empty() {
                format!("gemini image score {:.1}", utility_score)
            } else {
                response.raw
            },
        })
    }
}

#[async_trait::async_trait]
impl SeedRecordJudge for GeminiImageSeedRecordJudge {
    async fn judge_records(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        task: &DatasetSelectionTask,
        _sample: &DownloadedSample,
        records: &[SampleRecord],
    ) -> Result<SeedRecordJudgeReport> {
        let mut scored_records = Vec::with_capacity(records.len());
        for record in records {
            scored_records.push(self.judge_record(result, metadata, task, record).await?);
        }

        Ok(SeedRecordJudgeReport {
            summary: format!(
                "scored {} image seed records via Gemini image evaluation API",
                scored_records.len()
            ),
            scored_records,
        })
    }
}

/// DeepSeek-backed judge for the random seed subset.
pub struct DeepSeekSeedRecordJudge {
    llm: DeepSeekJsonClient,
    max_record_chars: usize,
}

impl DeepSeekSeedRecordJudge {
    pub fn from_env() -> Self {
        Self::new(IntentParserConfig::from_env())
    }

    pub fn new(config: IntentParserConfig) -> Self {
        Self {
            llm: DeepSeekJsonClient::new(config),
            max_record_chars: 800,
        }
    }

    pub fn with_record_char_limit(mut self, max_record_chars: usize) -> Self {
        self.max_record_chars = max_record_chars.max(64);
        self
    }
}

#[async_trait::async_trait]
impl SeedRecordJudge for DeepSeekSeedRecordJudge {
    async fn judge_records(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        task: &DatasetSelectionTask,
        sample: &DownloadedSample,
        records: &[SampleRecord],
    ) -> Result<SeedRecordJudgeReport> {
        self.llm
            .chat_json(
                SEED_RECORD_JUDGE_SYSTEM_PROMPT,
                build_seed_record_judge_user_prompt(
                    result,
                    metadata,
                    task,
                    sample,
                    records,
                    self.max_record_chars,
                )?,
                640,
            )
            .await
    }
}

struct DeepSeekJsonClient {
    client: Client,
    config: IntentParserConfig,
}

impl DeepSeekJsonClient {
    fn new(config: IntentParserConfig) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(config.timeout)
            .user_agent("guixu/0.1")
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client, config }
    }

    async fn chat_json<T: DeserializeOwned>(
        &self,
        system_prompt: &str,
        user_prompt: String,
        max_tokens: u32,
    ) -> Result<T> {
        let api_key = self
            .config
            .api_key
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("missing DEEPSEEK_API_KEY"))?;
        let endpoint = format!(
            "{}/chat/completions",
            self.config.api_base.trim_end_matches('/')
        );
        let request = DeepSeekChatRequest {
            model: self.config.model.clone(),
            messages: vec![
                DeepSeekMessage {
                    role: "system",
                    content: system_prompt.to_string(),
                },
                DeepSeekMessage {
                    role: "user",
                    content: user_prompt,
                },
            ],
            response_format: DeepSeekResponseFormat {
                kind: "json_object".to_string(),
            },
            temperature: 0.0,
            max_tokens,
            stream: false,
        };

        let response = self
            .client
            .post(endpoint)
            .bearer_auth(api_key)
            .json(&request)
            .send()
            .await
            .context("send DeepSeek sample evaluation request")?
            .error_for_status()
            .context("DeepSeek sample evaluation returned error status")?
            .json::<DeepSeekChatResponse>()
            .await
            .context("parse DeepSeek sample evaluation response")?;
        let content = response
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .map(|content| content.trim().to_string())
            .filter(|content| !content.is_empty())
            .ok_or_else(|| anyhow!("DeepSeek returned an empty sample-evaluation payload"))?;
        serde_json::from_str(&content)
            .with_context(|| format!("parse DeepSeek sample evaluation JSON: {content}"))
    }
}

#[derive(Debug, Serialize)]
struct DeepSeekChatRequest {
    model: String,
    messages: Vec<DeepSeekMessage>,
    response_format: DeepSeekResponseFormat,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct DeepSeekMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Serialize)]
struct DeepSeekResponseFormat {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct DeepSeekChatResponse {
    choices: Vec<DeepSeekChoice>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekChoice {
    message: DeepSeekChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct DeepSeekChoiceMessage {
    content: Option<String>,
}

#[derive(Debug, Serialize)]
struct GeminiImageEvalRequest {
    task: String,
    image: GeminiImageEvalImage,
}

#[derive(Debug, Serialize)]
struct GeminiImageEvalImage {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct GeminiImageEvalResponse {
    result: serde_json::Value,
    #[serde(default)]
    raw: String,
}

const SEED_RECORD_JUDGE_SYSTEM_PROMPT: &str = r#"You score individual sample records from a candidate dataset.
Return valid json only with this schema:
{
  "summary": "string",
  "scored_records": [
    {
      "record_id": "string",
      "utility_score": 0,
      "rationale": "string"
    }
  ]
}

Rules:
- utility_score must be 0-100.
- Higher means the record is strong evidence that the dataset will help the task.
- Lower means the record looks off-task, weakly labelled, noisy, or otherwise unhelpful.
- Score every provided record exactly once.
- record_id must exactly match one of the provided record ids.
"#;

const REQUIREMENTS_SYSTEM_PROMPT: &str = r#"You help plan sample-based dataset evaluation.
Return valid json only with this schema:
{
  "summary": "string",
  "required_signals": ["lowercase phrase"],
  "preferred_labels": ["lowercase phrase"],
  "disqualifying_signals": ["lowercase phrase"]
}

Rules:
- summary should describe the evidence a useful sample should contain.
- required_signals should be short phrases that a local proxy can match against sample content.
- preferred_labels should list label names or concepts that should appear in sample annotations.
- disqualifying_signals should contain short phrases that strongly suggest the sample is off-task.
- Keep arrays short and practical for cheap local screening.
"#;

const SAMPLE_JUDGE_SYSTEM_PROMPT: &str = r#"You score whether a small sample suggests a dataset will help a task.
Return valid json only with this schema:
{
  "utility_score": 0,
  "rationale": "string"
}

Rules:
- utility_score must be 0-100.
- Higher means the sample strongly suggests the full dataset is useful.
- Consider the task, candidate dataset metadata, requirement plan, proxy screening result, and sample preview.
- Penalize samples that are off-task, weakly labelled, low-signal, or inconsistent with the task.
"#;

fn build_requirements_user_prompt(task: &DatasetSelectionTask) -> String {
    format!(
        "Task description:\n{}\n\nTask type: {}\nTarget entity: {}\nRequired columns: {}\nRequired data type: {:?}\n",
        task.task_description,
        task.task_type,
        task.target_entity.as_deref().unwrap_or("<none>"),
        if task.required_columns.is_empty() {
            "<none>".to_string()
        } else {
            task.required_columns.join(", ")
        },
        task.required_data_type
    )
}

fn build_seed_record_judge_user_prompt(
    result: &SearchResult,
    metadata: &DatasetMetadata,
    task: &DatasetSelectionTask,
    sample: &DownloadedSample,
    records: &[SampleRecord],
    max_record_chars: usize,
) -> Result<String> {
    let preview = records
        .iter()
        .map(|record| {
            serde_json::json!({
                "id": record.id.as_str(),
                "content": truncate_for_prompt(&record.content, max_record_chars),
                "metadata": &record.metadata,
            })
        })
        .collect::<Vec<_>>();

    Ok(format!(
        "Task:\n{}\n\nCandidate dataset:\n{}\n\nResolved metadata:\n{}\n\nDownloaded sample summary:\n{}\n\nRecords to score:\n{}\n",
        serde_json::to_string_pretty(task)?,
        serde_json::to_string_pretty(result)?,
        serde_json::to_string_pretty(metadata)?,
        serde_json::to_string_pretty(&serde_json::json!({
            "sampled_rows": sample.sampled_rows,
            "sampled_bytes": sample.sampled_bytes,
            "summary": sample.summary.clone(),
        }))?,
        serde_json::to_string_pretty(&preview)?,
    ))
}

fn build_gemini_image_eval_task(
    result: &SearchResult,
    metadata: &DatasetMetadata,
    task: &DatasetSelectionTask,
    record: &SampleRecord,
    max_record_chars: usize,
) -> String {
    format!(
        "Task description: {}\nTask type: {}\nCandidate dataset: {}\nDataset data type: {:?}\nTarget entity: {}\nRecord annotation/context: {}\nEvaluate the attached image itself. Return only a number from 0 to 100, where 100 means this image is highly relevant and useful for selecting this dataset for the task, and 0 means it is off-task, ambiguous, or low-signal. Reply with only a number.",
        task.task_description,
        task.task_type,
        result.title,
        metadata.data_type,
        task.target_entity.as_deref().unwrap_or("<none>"),
        truncate_for_prompt(&record.content, max_record_chars),
    )
}

fn build_sample_judge_user_prompt(
    result: &SearchResult,
    metadata: &DatasetMetadata,
    task: &DatasetSelectionTask,
    requirements: &SampleRequirements,
    sample: &DownloadedSample,
    screening: &ProxyScreeningReport,
    max_preview_records: usize,
) -> Result<String> {
    let preview = sample
        .records
        .iter()
        .take(max_preview_records)
        .map(|record| {
            serde_json::json!({
                "id": record.id.as_str(),
                "content": record.content.as_str(),
                "metadata": &record.metadata,
            })
        })
        .collect::<Vec<_>>();

    Ok(format!(
        "Task:\n{}\n\nCandidate dataset:\n{}\n\nResolved metadata:\n{}\n\nRequirement plan:\n{}\n\nProxy screening:\n{}\n\nSample preview:\n{}\n",
        serde_json::to_string_pretty(task)?,
        serde_json::to_string_pretty(result)?,
        serde_json::to_string_pretty(metadata)?,
        serde_json::to_string_pretty(requirements)?,
        serde_json::to_string_pretty(screening)?,
        serde_json::to_string_pretty(&preview)?,
    ))
}

fn truncate_for_prompt(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    text.chars().take(max_chars).collect::<String>()
}

fn normalize_gemini_eval_api_url(api_url: String) -> String {
    if api_url.trim().is_empty() {
        "http://127.0.0.1:3000/api/search/gemini".into()
    } else if api_url.starts_with("http://") || api_url.starts_with("https://") {
        api_url
    } else if api_url.starts_with('/') {
        format!("http://127.0.0.1:3000{api_url}")
    } else {
        api_url
    }
}

fn sample_record_image_path(record: &SampleRecord) -> Option<PathBuf> {
    record
        .metadata
        .get("local_image_path")
        .and_then(|value| value.as_str())
        .map(PathBuf::from)
}

fn parse_gemini_numeric_result(response: &GeminiImageEvalResponse) -> Option<f64> {
    match &response.result {
        serde_json::Value::Number(value) => value.as_f64(),
        serde_json::Value::String(value) => parse_numeric_text(value),
        _ => parse_numeric_text(&response.raw),
    }
    .or_else(|| parse_numeric_text(&response.raw))
}

fn parse_numeric_text(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok().or_else(|| {
        trimmed
            .split(|character: char| {
                !(character.is_ascii_digit() || character == '.' || character == '-' || character == '+')
            })
            .find(|token| token.chars().any(|character| character.is_ascii_digit()))
            .and_then(|token| token.parse::<f64>().ok())
    })
}
