use anyhow::Result;
use data_core::feedback::CommunitySignal;
use data_core::metadata::DatasetMetadata;
use data_core::types::{DataType, SearchResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::warn;

use crate::adapters::ExternalAdapter;
use crate::intent::{IntentParser, QueryProfile};
use crate::vector_index::VectorIndex;

/// Search filters that can be applied to results.
#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    pub topic: Option<String>,
    pub min_rows: Option<u64>,
    pub max_price: Option<f64>,
    pub license: Option<String>,
    pub min_quality: Option<f64>,
    pub source: Option<String>,
}

/// Callback to fetch community signal for a dataset CID.
/// Allows the search engine to rank by TCV without owning the feedback store.
pub type SignalFetcher = Box<dyn Fn(&str) -> CommunitySignal + Send + Sync>;

/// The unified search engine. Merges results from local store, DHT,
/// and external adapters (Kaggle, HuggingFace, IPFS, PostgreSQL, DuckDB).
pub struct SearchEngine {
    #[allow(dead_code)]
    vector_index: VectorIndex,
    intent_parser: IntentParser,
    adapters: Vec<Box<dyn ExternalAdapter>>,
}

impl SearchEngine {
    pub fn new(
        vector_index: VectorIndex,
        intent_parser: IntentParser,
        adapters: Vec<Box<dyn ExternalAdapter>>,
    ) -> Self {
        Self {
            vector_index,
            intent_parser,
            adapters,
        }
    }

    /// Main search entry point — called by MCP tool `dataset_search`.
    /// `local_metadata` comes from the RocksDB store (P2P-discovered datasets).
    /// `signal_fetcher` retrieves on-chain community feedback for TCV ranking.
    pub async fn search(
        &self,
        query: &str,
        filters: &SearchFilters,
        local_metadata: &[DatasetMetadata],
        signal_fetcher: &SignalFetcher,
        limit: usize,
    ) -> Result<SearchOutput> {
        self.search_with_task_type(query, None, filters, local_metadata, signal_fetcher, limit)
            .await
    }

    /// Search entry point with an optional task-type override supplied by the caller.
    pub async fn search_with_task_type(
        &self,
        query: &str,
        task_type: Option<&str>,
        filters: &SearchFilters,
        local_metadata: &[DatasetMetadata],
        signal_fetcher: &SignalFetcher,
        limit: usize,
    ) -> Result<SearchOutput> {
        let profile = self.intent_parser.profile(query).await?;
        let profile = profile_with_task_type(profile, task_type);
        let mut output = self
            .search_with_profile(&profile, filters, local_metadata, signal_fetcher, limit)
            .await?;
        output.profile = Some(profile);
        Ok(output)
    }

    /// Search entry point when the caller already has a structured query profile.
    ///
    /// This keeps the existing search behaviour intact while exposing a stable
    /// seam for unit tests and future profiler implementations.
    pub async fn search_with_profile(
        &self,
        profile: &QueryProfile,
        filters: &SearchFilters,
        local_metadata: &[DatasetMetadata],
        signal_fetcher: &SignalFetcher,
        limit: usize,
    ) -> Result<SearchOutput> {
        // Parallel: local P2P store + all external adapters
        let (local_results, (external_results, errors)) = tokio::join!(
            self.search_local(profile, filters, local_metadata),
            self.search_external(profile, filters, limit),
        );

        let local_metadata_by_cid: HashMap<&str, &DatasetMetadata> = local_metadata
            .iter()
            .map(|metadata| (metadata.cid.0.as_str(), metadata))
            .collect();
        let mut all: Vec<SearchResult> = local_results.unwrap_or_default();
        all.extend(external_results);

        // Apply filters
        if let Some(ref topic) = filters.topic {
            let topic = topic.to_lowercase();
            all.retain(|r| searchable_result_text(r).contains(&topic));
        }
        if let Some(min_rows) = filters.min_rows {
            all.retain(|r| r.schema.row_count >= min_rows);
        }
        if let Some(max_price) = filters.max_price {
            all.retain(|r| r.price.amount <= max_price);
        }
        if let Some(ref license) = filters.license {
            let license = license.to_lowercase();
            all.retain(|r| r.license.spdx_id.to_lowercase() == license);
        }
        if let Some(min_quality) = filters.min_quality {
            all.retain(|r| {
                r.quality
                    .as_ref()
                    .map(|quality| quality.total >= min_quality)
                    .unwrap_or(false)
            });
        }
        if let Some(ref src) = filters.source {
            all.retain(|r| {
                format!("{:?}", r.source)
                    .to_lowercase()
                    .contains(&src.to_lowercase())
            });
        }

        apply_intent_hard_filters(&mut all, profile, &local_metadata_by_cid);

        // Deduplicate by CID
        let mut seen = HashSet::new();
        all.retain(|r| seen.insert(r.cid.0.clone()));

        // Rank with community signal (TCV-lite for search ranking)
        let mut ranked: Vec<RankedResult> = all
            .into_iter()
            .map(|r| {
                let metadata = local_metadata_by_cid.get(r.cid.0.as_str()).copied();
                let signal = signal_fetcher(&r.cid.0);
                let score = rank_with_signal(&r, metadata, &signal, profile);
                RankedResult {
                    result: r,
                    rank_score: score,
                    signal,
                }
            })
            .collect();

        ranked.sort_by(|a, b| {
            b.rank_score
                .partial_cmp(&a.rank_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked.truncate(limit);

        Ok(SearchOutput {
            results: ranked,
            errors,
            profile: None,
        })
    }

    /// Convenience entry point for "search, then value, then pick the best dataset".
    ///
    /// The first stage uses only the task description plus dataset metadata/search
    /// result fields to produce a coarse ranking. The second stage optionally
    /// resolves richer metadata for external results, plans a bandwidth-aware
    /// sample, and asks a caller-provided proxy evaluator to score the top-k
    /// candidates without downloading each full dataset.
    pub async fn search_and_value(
        &self,
        query: &str,
        filters: &SearchFilters,
        local_metadata: &[DatasetMetadata],
        signal_fetcher: &SignalFetcher,
        metadata_resolver: Option<&dyn MetadataResolver>,
        sample_evaluator: Option<&dyn SampleEvaluator>,
        task: Option<&DatasetSelectionTask>,
        config: &DatasetValuationConfig,
        limit: usize,
    ) -> Result<DatasetSelectionOutput> {
        let profile = self.intent_parser.profile(query).await?;
        let task = task
            .cloned()
            .unwrap_or_else(|| DatasetSelectionTask::from(&profile));
        let search_output = self
            .search_with_profile(&profile, filters, local_metadata, signal_fetcher, limit)
            .await?;

        self.value_search_output(
            &search_output,
            local_metadata,
            metadata_resolver,
            sample_evaluator,
            &task,
            config,
        )
        .await
    }

    /// Value already-discovered search results for a specific training task.
    ///
    /// This method is intentionally additive: it leaves the existing search API
    /// intact while exposing a higher-level selection primitive for the
    /// "search first, then estimate value, then download only the winner" flow.
    pub async fn value_search_output(
        &self,
        search_output: &SearchOutput,
        local_metadata: &[DatasetMetadata],
        metadata_resolver: Option<&dyn MetadataResolver>,
        sample_evaluator: Option<&dyn SampleEvaluator>,
        task: &DatasetSelectionTask,
        config: &DatasetValuationConfig,
    ) -> Result<DatasetSelectionOutput> {
        struct CandidateState {
            valuation: DatasetValuation,
            metadata: Option<DatasetMetadata>,
        }

        let mut errors = search_output.errors.clone();
        let local_metadata_by_cid: HashMap<&str, &DatasetMetadata> = local_metadata
            .iter()
            .map(|metadata| (metadata.cid.0.as_str(), metadata))
            .collect();

        let mut candidates = Vec::with_capacity(search_output.results.len());

        for ranked in &search_output.results {
            let mut metadata = local_metadata_by_cid
                .get(ranked.result.cid.0.as_str())
                .map(|metadata| (*metadata).clone());

            if metadata.is_none() {
                if let Some(resolver) = metadata_resolver {
                    match resolver.resolve_metadata(&ranked.result).await {
                        Ok(resolved) => metadata = resolved,
                        Err(error) => errors.push(format!(
                            "metadata resolve failed for {}: {error}",
                            ranked.result.cid.0
                        )),
                    }
                }
            }

            if let Some(required_data_type) = task.required_data_type {
                let candidate_data_type = metadata
                    .as_ref()
                    .map(|metadata| metadata.data_type)
                    .unwrap_or(ranked.result.data_type);
                if candidate_data_type != required_data_type {
                    continue;
                }
            }

            let task_similarity = compute_task_similarity(task, &ranked.result, metadata.as_ref());
            let schema_fit = compute_schema_fit(task, &ranked.result, metadata.as_ref());
            let scale_score = compute_scale_score(config, &ranked.result, metadata.as_ref());
            let balance_score = compute_balance_score(task, &ranked.result, metadata.as_ref());
            let metadata_quality = compute_metadata_quality(&ranked.result, metadata.as_ref());
            let coarse_score = compose_coarse_score(
                task_similarity,
                schema_fit,
                scale_score,
                balance_score,
                metadata_quality,
            );

            candidates.push(CandidateState {
                metadata: metadata.clone(),
                valuation: DatasetValuation {
                    result: ranked.result.clone(),
                    coarse_score,
                    final_score: coarse_score,
                    task_similarity,
                    schema_fit,
                    scale_score,
                    balance_score,
                    metadata_quality,
                    metadata_resolved: metadata.is_some(),
                    sample_plan: None,
                    proxy_utility: None,
                    explanation: String::new(),
                },
            });
        }

        candidates.sort_by(|a, b| {
            b.valuation
                .coarse_score
                .partial_cmp(&a.valuation.coarse_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let shortlist = config.coarse_top_k.min(candidates.len());
        for candidate in candidates.iter_mut().take(shortlist) {
            let Some(sample_evaluator) = sample_evaluator else {
                break;
            };
            let Some(metadata) = candidate.metadata.as_ref() else {
                errors.push(format!(
                    "sample evaluation skipped for {}: metadata unavailable",
                    candidate.valuation.result.cid.0
                ));
                continue;
            };

            let plan = build_sample_plan(metadata, config);
            if plan.estimated_rows == 0 || plan.sample_fraction == 0.0 {
                continue;
            }

            candidate.valuation.sample_plan = Some(plan.clone());
            match sample_evaluator
                .evaluate_sample(&candidate.valuation.result, metadata, task, &plan)
                .await
            {
                Ok(Some(proxy_utility)) => {
                    candidate.valuation.final_score = match proxy_utility.apply_mode {
                        ProxyUtilityApplyMode::Blend => blend_scores(
                            candidate.valuation.coarse_score,
                            proxy_utility.utility_score,
                            config.metadata_weight,
                            config.utility_weight,
                        ),
                        ProxyUtilityApplyMode::OverrideFinal => proxy_utility.utility_score,
                    };
                    candidate.valuation.proxy_utility = Some(proxy_utility);
                }
                Ok(None) => {}
                Err(error) => errors.push(format!(
                    "sample evaluation failed for {}: {error}",
                    candidate.valuation.result.cid.0
                )),
            }
        }

        for candidate in &mut candidates {
            candidate.valuation.explanation = build_valuation_explanation(&candidate.valuation);
        }

        candidates.sort_by(|a, b| {
            b.valuation
                .final_score
                .partial_cmp(&a.valuation.final_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let ranked_candidates: Vec<DatasetValuation> = candidates
            .into_iter()
            .map(|candidate| candidate.valuation)
            .collect();
        let selected = ranked_candidates.first().cloned();

        Ok(DatasetSelectionOutput {
            task: task.clone(),
            selected,
            candidates: ranked_candidates,
            errors,
        })
    }

    /// Search local P2P metadata store.
    async fn search_local(
        &self,
        intent: &ParsedIntent,
        _filters: &SearchFilters,
        metadata: &[DatasetMetadata],
    ) -> Result<Vec<SearchResult>> {
        let keyword_terms = normalized_intent_keywords(intent);
        let fallback_query = intent.raw_query.to_lowercase();

        let results: Vec<SearchResult> = metadata
            .iter()
            .filter(|m| {
                let title = m.title.to_lowercase();
                let desc = m.description.as_deref().unwrap_or("").to_lowercase();
                let tags_str = m.tags.join(" ").to_lowercase();
                let column_names = m
                    .schema
                    .columns
                    .iter()
                    .map(|column| column.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
                let data_type = format!("{:?}", m.data_type).to_lowercase();
                let all_text = format!("{title} {desc} {tags_str} {column_names} {data_type}");

                if keyword_terms.is_empty() {
                    all_text.contains(&fallback_query)
                } else {
                    keyword_terms
                        .iter()
                        .any(|keyword| all_text.contains(keyword))
                }
            })
            .map(metadata_to_search_result)
            .collect();

        Ok(results)
    }

    /// Search external platforms via adapters.
    async fn search_external(
        &self,
        intent: &ParsedIntent,
        _filters: &SearchFilters,
        limit: usize,
    ) -> (Vec<SearchResult>, Vec<String>) {
        let mut results = vec![];
        let mut errors = vec![];
        let external_query = build_search_query(intent);
        for adapter in &self.adapters {
            match adapter.search(&external_query, limit).await {
                Ok(mut r) => results.append(&mut r),
                Err(e) => {
                    warn!(adapter = adapter.name(), error = %e, "adapter search failed");
                    errors.push(format!("{}: {e}", adapter.name()));
                }
            }
        }
        (results, errors)
    }
}

/// A search result annotated with ranking score and community signal.
#[derive(Debug, Clone, Serialize)]
pub struct RankedResult {
    pub result: SearchResult,
    pub rank_score: f64,
    pub signal: CommunitySignal,
}

/// Search output including results and any adapter errors.
#[derive(Debug, Serialize)]
pub struct SearchOutput {
    pub results: Vec<RankedResult>,
    pub errors: Vec<String>,
    /// The structured intent profile derived from the query (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<QueryProfile>,
}

/// Structured description of the downstream training task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSelectionTask {
    pub task_description: String,
    pub task_type: String,
    pub required_columns: Vec<String>,
    pub target_entity: Option<String>,
    pub required_data_type: Option<DataType>,
}

impl From<&QueryProfile> for DatasetSelectionTask {
    fn from(profile: &QueryProfile) -> Self {
        Self {
            task_description: profile
                .task_description
                .clone()
                .filter(|description| !description.trim().is_empty())
                .unwrap_or_else(|| profile.raw_query.clone()),
            task_type: profile
                .task_type
                .clone()
                .unwrap_or_else(|| "general".to_string()),
            required_columns: infer_required_columns(profile),
            target_entity: profile.target_entity.clone(),
            required_data_type: sample_unit_data_type(&profile.data_standard.sample_unit)
                .or_else(|| strict_task_data_type(profile.task_type.as_deref())),
        }
    }
}

/// Knobs for the two-stage dataset valuation pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetValuationConfig {
    pub coarse_top_k: usize,
    pub sample_fraction: f64,
    pub max_sample_rows: u64,
    pub max_sample_bytes: u64,
    pub sample_time_budget_secs: u64,
    pub metadata_weight: f64,
    pub utility_weight: f64,
    pub scale_saturation_rows: u64,
    pub scale_saturation_bytes: u64,
}

impl Default for DatasetValuationConfig {
    fn default() -> Self {
        Self {
            coarse_top_k: 5,
            sample_fraction: 0.01,
            max_sample_rows: 2_000,
            max_sample_bytes: 64 * 1024 * 1024,
            sample_time_budget_secs: 120,
            metadata_weight: 0.65,
            utility_weight: 0.35,
            scale_saturation_rows: 100_000,
            scale_saturation_bytes: 512 * 1024 * 1024,
        }
    }
}

/// Download/train budget for the optional sample stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplePlan {
    pub sample_fraction: f64,
    pub estimated_rows: u64,
    pub estimated_bytes: u64,
    pub max_rows: u64,
    pub max_bytes: u64,
    pub time_budget_secs: u64,
}

/// How a sample-based utility score should affect the final dataset valuation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProxyUtilityApplyMode {
    Blend,
    OverrideFinal,
}

/// Proxy model result computed from a small sample of a dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyUtilityReport {
    pub utility_score: f64,
    pub apply_mode: ProxyUtilityApplyMode,
    pub proxy_metric_name: String,
    pub proxy_metric_value: f64,
    pub sampled_rows: u64,
    pub sampled_bytes: u64,
    pub notes: Option<String>,
}

/// Full valuation record for a candidate dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetValuation {
    pub result: SearchResult,
    pub coarse_score: f64,
    pub final_score: f64,
    pub task_similarity: f64,
    pub schema_fit: f64,
    pub scale_score: f64,
    pub balance_score: f64,
    pub metadata_quality: f64,
    pub metadata_resolved: bool,
    pub sample_plan: Option<SamplePlan>,
    pub proxy_utility: Option<ProxyUtilityReport>,
    pub explanation: String,
}

/// End-to-end output for search + valuation + best-dataset selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSelectionOutput {
    pub task: DatasetSelectionTask,
    pub selected: Option<DatasetValuation>,
    pub candidates: Vec<DatasetValuation>,
    pub errors: Vec<String>,
}

/// Allows callers to resolve richer metadata for external search results.
#[async_trait::async_trait]
pub trait MetadataResolver: Send + Sync {
    async fn resolve_metadata(&self, result: &SearchResult) -> Result<Option<DatasetMetadata>>;
}

/// Optional second-stage evaluator backed by a tiny proxy model.
#[async_trait::async_trait]
pub trait SampleEvaluator: Send + Sync {
    async fn evaluate_sample(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        task: &DatasetSelectionTask,
        plan: &SamplePlan,
    ) -> Result<Option<ProxyUtilityReport>>;
}

/// Backward-compatible alias for the structured query profile type.
pub type ParsedIntent = QueryProfile;

/// Rank a search result incorporating community signal.
/// This is a lightweight version of TCV used for search ranking.
fn rank_with_signal(
    result: &SearchResult,
    metadata: Option<&DatasetMetadata>,
    signal: &CommunitySignal,
    intent: &ParsedIntent,
) -> f64 {
    let quality = result.quality.as_ref().map(|q| q.total).unwrap_or(50.0);

    let metadata_text = build_dataset_text(result, metadata).to_lowercase();
    let searchable_text = searchable_result_text(result);
    let task_text = intent
        .task_description
        .as_deref()
        .filter(|description| !description.trim().is_empty())
        .unwrap_or(intent.raw_query.as_str());

    let keywords = normalized_intent_keywords(intent);
    let keyword_hits = keywords
        .iter()
        .filter(|keyword| {
            metadata_text.contains(keyword.as_str()) || searchable_text.contains(keyword.as_str())
        })
        .count();
    let keyword_relevance = if keywords.is_empty() {
        0.0
    } else {
        (keyword_hits as f64 / keywords.len() as f64) * 100.0
    };
    let task_description_relevance = cosine_similarity(task_text, &metadata_text) * 100.0;
    let relevance = if keywords.is_empty() {
        task_description_relevance
    } else {
        (0.35 * keyword_relevance + 0.65 * task_description_relevance).clamp(0.0, 100.0)
    };

    // Community signal: positive reviews boost, negative reviews penalize
    let community = if signal.total_reviews > 0 {
        let base = signal.avg_relevance * 50.0 + 50.0; // map [-1,1] → [0,100]
        let confidence = 1.0 - (1.0 / (1.0 + signal.total_reviews as f64 * 0.2));
        50.0 * (1.0 - confidence) + base * confidence
    } else {
        50.0 // neutral when no reviews
    };

    let risk = signal.risk_penalty();
    let price_penalty = if result.price.is_free() { 0.0 } else { 5.0 };
    let market_boost = market_signal_score(result);
    let task_type_adjustment = match required_data_type(intent) {
        Some(expected_type) if result.data_type == expected_type => 20.0,
        Some(_) => -40.0,
        None => 0.0,
    };

    // Weighted combination
    0.30 * relevance
        + 0.25 * quality
        + 0.25 * community
        + 0.15 * 70.0 /* freshness placeholder */
        + 0.05 * market_boost
        - 0.05 * risk
        - price_penalty
        + task_type_adjustment
}

fn normalized_intent_keywords(intent: &ParsedIntent) -> Vec<String> {
    let mut seen = HashSet::new();
    intent
        .keywords
        .iter()
        .map(|keyword| keyword.trim().to_lowercase())
        .filter(|keyword| !keyword.is_empty())
        .filter(|keyword| seen.insert(keyword.clone()))
        .collect()
}

fn build_search_query(intent: &ParsedIntent) -> String {
    let keywords = normalized_intent_keywords(intent);
    if keywords.is_empty() {
        intent.raw_query.clone()
    } else {
        keywords.join(" ")
    }
}

fn profile_with_task_type(mut profile: QueryProfile, task_type: Option<&str>) -> QueryProfile {
    if let Some(task_type) = task_type.map(str::trim).filter(|s| !s.is_empty()) {
        profile.task_type = Some(task_type.to_string());
    }
    profile
}

fn apply_intent_hard_filters(
    results: &mut Vec<SearchResult>,
    profile: &QueryProfile,
    metadata_by_cid: &HashMap<&str, &DatasetMetadata>,
) {
    if let Some(required_type) = required_data_type(profile) {
        results.retain(|result| result.data_type == required_type);
    }

    if let Some(required_resolution) = required_min_resolution(profile) {
        results.retain(|result| {
            let metadata = metadata_by_cid.get(result.cid.0.as_str()).copied();
            candidate_resolution(result, metadata)
                .map(|resolution| resolution.satisfies(&required_resolution))
                .unwrap_or(false)
        });
    }
}

fn required_data_type(profile: &QueryProfile) -> Option<data_core::types::DataType> {
    sample_unit_data_type(&profile.data_standard.sample_unit)
        .or_else(|| strict_task_data_type(profile.task_type.as_deref()))
}

fn sample_unit_data_type(sample_unit: &str) -> Option<data_core::types::DataType> {
    use data_core::types::DataType;

    match sample_unit.trim().to_lowercase().as_str() {
        "image" | "images" | "photo" | "photos" | "picture" | "pictures" => Some(DataType::Image),
        "video" | "videos" => Some(DataType::Video),
        "audio" => Some(DataType::Audio),
        "text" | "document" | "documents" => Some(DataType::Text),
        "tabular" | "table" | "tables" | "csv" => Some(DataType::Tabular),
        _ => None,
    }
}

fn strict_task_data_type(task_type: Option<&str>) -> Option<data_core::types::DataType> {
    use data_core::types::DataType;

    match task_type.map(|s| s.trim().to_lowercase()) {
        Some(task_type)
            if matches!(
                task_type.as_str(),
                "time_series_prediction" | "forecasting" | "regression"
            ) =>
        {
            Some(DataType::Tabular)
        }
        Some(task_type) if task_type == "nlp" => Some(DataType::Text),
        Some(task_type) if task_type == "video_classification" => Some(DataType::Video),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolutionConstraint {
    min_long_side: Option<u32>,
    min_short_side: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SpatialResolution {
    long_side: u32,
    short_side: u32,
}

impl SpatialResolution {
    fn from_dimensions(width: u32, height: u32) -> Option<Self> {
        let long_side = width.max(height);
        let short_side = width.min(height);
        if long_side == 0 || short_side == 0 {
            None
        } else {
            Some(Self {
                long_side,
                short_side,
            })
        }
    }

    fn from_scan_height(height: u32) -> Option<Self> {
        if height == 0 {
            None
        } else {
            Some(Self {
                long_side: height,
                short_side: height,
            })
        }
    }

    fn satisfies(&self, constraint: &ResolutionConstraint) -> bool {
        self.short_side >= constraint.min_short_side
            && constraint
                .min_long_side
                .map(|min_long_side| self.long_side >= min_long_side)
                .unwrap_or(true)
    }
}

fn required_min_resolution(profile: &QueryProfile) -> Option<ResolutionConstraint> {
    profile
        .data_standard
        .metadata_fields
        .iter()
        .find(|field| field.name.eq_ignore_ascii_case("resolution"))
        .and_then(|field| parse_resolution_constraint(&field.value))
}

fn parse_resolution_constraint(value: &str) -> Option<ResolutionConstraint> {
    let value = value.trim().to_lowercase();
    if value.is_empty() {
        return None;
    }

    if let Some((left, right)) = split_resolution_dims(&value) {
        let width = left.parse::<u32>().ok()?;
        let height = right.parse::<u32>().ok()?;
        let long_side = width.max(height);
        let short_side = width.min(height);
        if long_side == 0 || short_side == 0 {
            return None;
        }
        return Some(ResolutionConstraint {
            min_long_side: Some(long_side),
            min_short_side: short_side,
        });
    }

    let digits = value
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if value.ends_with('p') {
        let short_side = digits.parse::<u32>().ok()?;
        if short_side == 0 {
            return None;
        }
        return Some(ResolutionConstraint {
            min_long_side: None,
            min_short_side: short_side,
        });
    }

    None
}

fn split_resolution_dims(value: &str) -> Option<(&str, &str)> {
    for separator in ['x', 'X', '*'] {
        if let Some((left, right)) = value.split_once(separator) {
            return Some((left.trim(), right.trim()));
        }
    }
    None
}

fn compose_coarse_score(
    task_similarity: f64,
    schema_fit: f64,
    scale_score: f64,
    balance_score: f64,
    metadata_quality: f64,
) -> f64 {
    (0.55 * task_similarity
        + 0.15 * schema_fit
        + 0.15 * scale_score
        + 0.10 * balance_score
        + 0.05 * metadata_quality)
        .clamp(0.0, 100.0)
}

fn blend_scores(
    metadata_score: f64,
    utility_score: f64,
    metadata_weight: f64,
    utility_weight: f64,
) -> f64 {
    let metadata_weight = metadata_weight.max(0.0);
    let utility_weight = utility_weight.max(0.0);
    let total_weight = (metadata_weight + utility_weight).max(f64::EPSILON);
    ((metadata_score * metadata_weight + utility_score * utility_weight) / total_weight)
        .clamp(0.0, 100.0)
}

fn compute_task_similarity(
    task: &DatasetSelectionTask,
    result: &SearchResult,
    metadata: Option<&DatasetMetadata>,
) -> f64 {
    let task_text = build_task_text(task);
    let dataset_text = build_dataset_text(result, metadata);
    let cosine = cosine_similarity(&task_text, &dataset_text) * 100.0;
    let dataset_lower = dataset_text.to_lowercase();

    let entity_bonus = task
        .target_entity
        .as_deref()
        .map(|entity| {
            if dataset_lower.contains(&entity.to_lowercase()) {
                15.0
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);

    let type_bonus = match task.task_type.as_str() {
        "classification" => {
            if dataset_lower.contains("classification")
                || dataset_lower.contains("classifier")
                || has_any_column_like(result, metadata, &["label", "class", "target", "category"])
            {
                10.0
            } else {
                0.0
            }
        }
        "forecasting" => {
            if dataset_lower.contains("forecast")
                || dataset_lower.contains("time series")
                || has_any_column_like(result, metadata, &["time", "date", "timestamp", "value"])
            {
                10.0
            } else {
                0.0
            }
        }
        _ => 0.0,
    };

    (cosine * 0.85 + entity_bonus + type_bonus).clamp(0.0, 100.0)
}

fn compute_schema_fit(
    task: &DatasetSelectionTask,
    result: &SearchResult,
    metadata: Option<&DatasetMetadata>,
) -> f64 {
    if task.required_columns.is_empty() {
        return 50.0;
    }

    let columns = candidate_column_names(result, metadata);
    if columns.is_empty() {
        return 25.0;
    }

    let total = task
        .required_columns
        .iter()
        .map(|required| {
            let required = required.to_lowercase();
            columns
                .iter()
                .map(|column| match_score(column, &required))
                .fold(0.0_f64, f64::max)
        })
        .sum::<f64>();

    (total / task.required_columns.len() as f64 * 100.0).clamp(0.0, 100.0)
}

fn compute_scale_score(
    config: &DatasetValuationConfig,
    result: &SearchResult,
    metadata: Option<&DatasetMetadata>,
) -> f64 {
    let (rows, bytes) = candidate_shape(result, metadata);

    let row_score = saturating_log_score(rows, config.scale_saturation_rows);
    let byte_score = saturating_log_score(bytes, config.scale_saturation_bytes);

    if rows == 0 && bytes == 0 {
        40.0
    } else {
        (0.7 * row_score + 0.3 * byte_score).clamp(0.0, 100.0)
    }
}

fn compute_balance_score(
    task: &DatasetSelectionTask,
    result: &SearchResult,
    metadata: Option<&DatasetMetadata>,
) -> f64 {
    let mut score = if task.task_type == "classification" {
        55.0
    } else {
        50.0
    };
    let dataset_text = build_dataset_text(result, metadata).to_lowercase();
    let row_count = candidate_shape(result, metadata).0;

    if task.task_type == "classification" {
        if has_any_column_like(result, metadata, &["label", "class", "target", "category"]) {
            score += 20.0;
        }
        if contains_any(
            &dataset_text,
            &["balanced", "class-balanced", "well-balanced", "stratified"],
        ) {
            score += 15.0;
        }
        if contains_any(&dataset_text, &["imbalanced", "long-tail", "skewed"]) {
            score -= 25.0;
        }
        if row_count > 5_000 {
            score += 5.0;
        } else if row_count > 0 && row_count < 500 {
            score -= 10.0;
        }
    }

    if let Some(stats) = metadata.and_then(|metadata| metadata.stats.as_ref()) {
        score += ((1.0 - stats.null_rate.clamp(0.0, 1.0)) * 10.0) - 5.0;
    }

    score.clamp(0.0, 100.0)
}

fn compute_metadata_quality(result: &SearchResult, metadata: Option<&DatasetMetadata>) -> f64 {
    let description_present =
        candidate_description(result, metadata).is_some() as u8 as f64 * 100.0;
    let column_count = candidate_column_names(result, metadata).len() as u64;
    let schema_score = if column_count == 0 {
        20.0
    } else {
        (40.0 + (column_count.min(8) as f64 / 8.0) * 60.0).clamp(0.0, 100.0)
    };
    let stats_score = metadata
        .and_then(|metadata| metadata.stats.as_ref())
        .map(|stats| (1.0 - stats.null_rate.clamp(0.0, 1.0)) * 100.0)
        .or_else(|| result.quality.as_ref().map(|quality| quality.total))
        .unwrap_or(50.0);
    let provenance_score = metadata
        .map(|metadata| {
            if !metadata.signature.is_empty() || metadata.verifiable_credential.is_some() {
                100.0
            } else {
                50.0
            }
        })
        .unwrap_or(50.0);

    (0.25 * description_present
        + 0.30 * schema_score
        + 0.30 * stats_score
        + 0.15 * provenance_score)
        .clamp(0.0, 100.0)
}

fn candidate_resolution(
    result: &SearchResult,
    metadata: Option<&DatasetMetadata>,
) -> Option<SpatialResolution> {
    metadata
        .and_then(|metadata| {
            metadata
                .video_meta
                .as_ref()
                .and_then(|video_meta| {
                    SpatialResolution::from_dimensions(video_meta.width, video_meta.height)
                })
                .or_else(|| parse_resolution_from_text(&build_dataset_text(result, Some(metadata))))
        })
        .or_else(|| parse_resolution_from_text(&build_dataset_text(result, None)))
}

fn parse_resolution_from_text(text: &str) -> Option<SpatialResolution> {
    let normalized = text.to_lowercase();
    let mut best = None;

    for token in normalized
        .split(|character: char| {
            !(character.is_ascii_alphanumeric() || matches!(character, 'x' | '*'))
        })
        .filter(|token| !token.is_empty())
    {
        let candidate = if let Some((left, right)) = split_resolution_dims(token) {
            match (left.parse::<u32>().ok(), right.parse::<u32>().ok()) {
                (Some(width), Some(height)) => SpatialResolution::from_dimensions(width, height),
                _ => None,
            }
        } else if let Some(height) = token
            .strip_suffix('p')
            .and_then(|value| value.parse::<u32>().ok())
        {
            SpatialResolution::from_scan_height(height)
        } else {
            None
        };

        if let Some(candidate) = candidate {
            if best
                .map(|current: SpatialResolution| {
                    candidate.long_side > current.long_side
                        || (candidate.long_side == current.long_side
                            && candidate.short_side > current.short_side)
                })
                .unwrap_or(true)
            {
                best = Some(candidate);
            }
        }
    }

    best
}

fn build_sample_plan(metadata: &DatasetMetadata, config: &DatasetValuationConfig) -> SamplePlan {
    if config.coarse_top_k == 0
        || config.sample_fraction <= 0.0
        || config.max_sample_rows == 0
        || config.max_sample_bytes == 0
    {
        return SamplePlan {
            sample_fraction: 0.0,
            estimated_rows: 0,
            estimated_bytes: 0,
            max_rows: config.max_sample_rows,
            max_bytes: config.max_sample_bytes,
            time_budget_secs: config.sample_time_budget_secs,
        };
    }

    let base_fraction = config.sample_fraction.clamp(0.0, 1.0);
    let row_fraction = if metadata.schema.row_count > 0 {
        config.max_sample_rows as f64 / metadata.schema.row_count as f64
    } else {
        1.0
    };
    let byte_fraction = if metadata.schema.size_bytes > 0 {
        config.max_sample_bytes as f64 / metadata.schema.size_bytes as f64
    } else {
        1.0
    };
    let sample_fraction = base_fraction
        .min(row_fraction)
        .min(byte_fraction)
        .clamp(0.0, 1.0);

    let estimated_rows = if metadata.schema.row_count == 0 || sample_fraction == 0.0 {
        0
    } else {
        ((metadata.schema.row_count as f64 * sample_fraction).ceil() as u64)
            .max(1)
            .min(config.max_sample_rows)
    };
    let estimated_bytes = if metadata.schema.size_bytes == 0 || sample_fraction == 0.0 {
        0
    } else {
        ((metadata.schema.size_bytes as f64 * sample_fraction).ceil() as u64)
            .max(1)
            .min(config.max_sample_bytes)
    };

    SamplePlan {
        sample_fraction,
        estimated_rows,
        estimated_bytes,
        max_rows: config.max_sample_rows,
        max_bytes: config.max_sample_bytes,
        time_budget_secs: config.sample_time_budget_secs,
    }
}

fn build_valuation_explanation(valuation: &DatasetValuation) -> String {
    let mut parts = vec![
        format!("coarse={:.1}", valuation.coarse_score),
        format!("task={:.1}", valuation.task_similarity),
        format!("schema={:.1}", valuation.schema_fit),
        format!("scale={:.1}", valuation.scale_score),
        format!("balance={:.1}", valuation.balance_score),
        format!("meta={:.1}", valuation.metadata_quality),
    ];

    if let Some(plan) = &valuation.sample_plan {
        parts.push(format!(
            "sample={:.2}% (~{} rows, {} bytes)",
            plan.sample_fraction * 100.0,
            plan.estimated_rows,
            plan.estimated_bytes
        ));
    }

    if let Some(proxy_utility) = &valuation.proxy_utility {
        parts.push(format!(
            "{}:{:?}={:.3} -> utility={:.1}",
            proxy_utility.proxy_metric_name,
            proxy_utility.apply_mode,
            proxy_utility.proxy_metric_value,
            proxy_utility.utility_score
        ));
    }

    parts.push(format!("final={:.1}", valuation.final_score));
    parts.join(", ")
}

fn infer_required_columns(profile: &QueryProfile) -> Vec<String> {
    match profile.task_type.as_deref() {
        Some("classification") => vec!["label".to_string()],
        Some("forecasting") => vec!["timestamp".to_string(), "value".to_string()],
        _ => vec![],
    }
}

fn build_task_text(task: &DatasetSelectionTask) -> String {
    let mut parts = vec![task.task_description.clone(), task.task_type.clone()];
    if !task.required_columns.is_empty() {
        parts.push(task.required_columns.join(" "));
    }
    if let Some(target_entity) = &task.target_entity {
        parts.push(target_entity.clone());
    }
    parts.join(" ")
}

fn build_dataset_text(result: &SearchResult, metadata: Option<&DatasetMetadata>) -> String {
    let mut parts = vec![result.title.clone()];
    if let Some(description) = result.description.as_deref() {
        parts.push(description.to_string());
    }

    if let Some(metadata) = metadata {
        parts.push(metadata.title.clone());
        if let Some(description) = metadata.description.as_deref() {
            parts.push(description.to_string());
        }
        if !metadata.tags.is_empty() {
            parts.push(metadata.tags.join(" "));
        }
        if !metadata.schema.columns.is_empty() {
            parts.push(
                metadata
                    .schema
                    .columns
                    .iter()
                    .map(|column| column.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
    } else if !result.schema.columns.is_empty() {
        parts.push(
            result
                .schema
                .columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        );
    }

    parts.join(" ")
}

fn candidate_description<'a>(
    result: &'a SearchResult,
    metadata: Option<&'a DatasetMetadata>,
) -> Option<&'a str> {
    metadata
        .and_then(|metadata| metadata.description.as_deref())
        .or(result.description.as_deref())
}

fn candidate_column_names(
    result: &SearchResult,
    metadata: Option<&DatasetMetadata>,
) -> Vec<String> {
    metadata
        .map(|metadata| {
            metadata
                .schema
                .columns
                .iter()
                .map(|column| column.name.to_lowercase())
                .collect()
        })
        .unwrap_or_else(|| {
            result
                .schema
                .columns
                .iter()
                .map(|column| column.name.to_lowercase())
                .collect()
        })
}

fn candidate_shape(result: &SearchResult, metadata: Option<&DatasetMetadata>) -> (u64, u64) {
    if let Some(metadata) = metadata {
        (metadata.schema.row_count, metadata.schema.size_bytes)
    } else {
        (result.schema.row_count, result.schema.size_bytes)
    }
}

fn has_any_column_like(
    result: &SearchResult,
    metadata: Option<&DatasetMetadata>,
    needles: &[&str],
) -> bool {
    let columns = candidate_column_names(result, metadata);
    needles.iter().any(|needle| {
        let needle = needle.to_lowercase();
        columns.iter().any(|column| {
            column == &needle || column.contains(&needle) || needle.contains(column.as_str())
        })
    })
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn saturating_log_score(value: u64, saturation: u64) -> f64 {
    if value == 0 {
        return 0.0;
    }
    if saturation <= 1 {
        return 100.0;
    }
    (((value as f64) + 1.0).ln() / ((saturation as f64) + 1.0).ln()).min(1.0) * 100.0
}

fn match_score(column: &str, required: &str) -> f64 {
    if column == required {
        1.0
    } else if column.contains(required) || required.contains(column) {
        0.6
    } else {
        0.0
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
        "a", "an", "and", "as", "at", "be", "build", "by", "data", "dataset", "for", "from",
        "high", "in", "into", "is", "of", "on", "or", "the", "to", "with",
    ];

    text.split(|c: char| !c.is_alphanumeric())
        .map(|token| token.to_lowercase())
        .filter(|token| token.len() > 1)
        .filter(|token| !STOPWORDS.contains(&token.as_str()))
        .collect()
}

/// Convert DatasetMetadata to SearchResult.
fn metadata_to_search_result(m: &DatasetMetadata) -> SearchResult {
    use data_core::types::DataSource;

    SearchResult {
        cid: m.cid.clone(),
        title: m.title.clone(),
        description: m.description.clone(),
        tags: m.tags.clone(),
        schema: m.schema.clone(),
        quality: None, // computed separately by TCV engine
        price: m.price.clone(),
        license: m.license.clone(),
        provider: m.provider.clone(),
        source: DataSource::P2p,
        market: None,
        data_type: m.data_type,
        created_at: m.created_at,
    }
}

fn searchable_result_text(result: &SearchResult) -> String {
    let columns = result
        .schema
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{} {} {} {} {}",
        result.title.to_lowercase(),
        result.description.as_deref().unwrap_or("").to_lowercase(),
        result.tags.join(" ").to_lowercase(),
        columns.to_lowercase(),
        format!("{:?}", result.data_type).to_lowercase()
    )
}

fn market_signal_score(result: &SearchResult) -> f64 {
    let Some(market) = &result.market else {
        return 0.0;
    };

    let downloads = (market.download_count as f64 + 1.0).ln();
    let reviews = (market.review_count as f64 + 1.0).ln();
    let trades = (market.trade_count as f64 + 1.0).ln();
    ((downloads * 20.0) + (reviews * 25.0) + (trades * 35.0)).clamp(0.0, 100.0)
}
