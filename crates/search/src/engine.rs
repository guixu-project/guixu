// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use data_attestation::{fetch_buyer_reviews, summarize_reviews, BaseChainClient, ChainConfig};
use data_core::feedback::CommunitySignal;
use data_core::metadata::DatasetMetadata;
use data_core::types::{
    DataSource, DataType, DatasetCid, SearchResult, SkillCapability, SourceFamily,
};
use futures::future::join_all;
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tracing::warn;

use crate::adapters::ExternalAdapter;
use crate::intent::{load_setting_env_value, IntentParser, QueryProfile};
use crate::vector_index::VectorIndex;

pub const ON_CHAIN_SCORE_NEUTRAL: f64 = 50.0;
pub const ON_CHAIN_COARSE_ADJUST_WEIGHT: f64 = 0.10;
const ON_CHAIN_TRADE_SATURATION: u64 = 50;
const ON_CHAIN_SENTIMENT_SAMPLE_LIMIT: usize = 20;
const MAX_SELECTION_BUDGET_BUCKETS: usize = 10_000;
const SELECTION_VALUE_EPSILON: f64 = 1e-6;
const ADAPTER_SEARCH_TIMEOUT_SECS: u64 = 5;

/// Search filters that can be applied to results.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchFilters {
    pub topic: Option<String>,
    pub min_rows: Option<u64>,
    pub max_price: Option<f64>,
    pub license: Option<String>,
    pub min_quality: Option<f64>,
    pub skill_ids: Vec<String>,
    pub source_families: Vec<SourceFamily>,
    pub required_capabilities: Vec<SkillCapability>,
    pub chain: Option<String>,
    pub protocol: Option<String>,
    pub asset: Option<String>,
    pub category: Option<String>,
    pub free_only: Option<bool>,
}

/// Callback to fetch community signal for a dataset CID.
/// Allows the search engine to rank by TCV without owning the feedback store.
pub type SignalFetcher = Box<dyn Fn(&str) -> CommunitySignal + Send + Sync>;

/// Input bundle for the higher-level "search, then value" workflow.
pub struct SearchAndValueRequest<'a> {
    pub query: &'a str,
    pub filters: &'a SearchFilters,
    pub local_metadata: &'a [DatasetMetadata],
    pub signal_fetcher: &'a SignalFetcher,
    pub metadata_resolver: Option<&'a dyn MetadataResolver>,
    pub sample_evaluator: Option<&'a dyn SampleEvaluator>,
    pub task: Option<&'a DatasetSelectionTask>,
    pub config: &'a DatasetValuationConfig,
    pub limit: usize,
}

/// Classifies buyer review comments into sentiment categories.
/// Implement this trait to provide LLM-based sentiment classification.
#[async_trait::async_trait]
pub trait SentimentClassifier: Send + Sync {
    async fn classify(
        &self,
        items: &[CommentClassificationItem],
    ) -> Result<HashMap<usize, ReviewSentiment>>;
}

/// The unified search engine. Merges results from local store, DHT,
/// and external adapters (Kaggle, HuggingFace, IPFS, PostgreSQL, DuckDB).
pub struct SearchEngine {
    #[allow(dead_code)]
    vector_index: VectorIndex,
    intent_parser: IntentParser,
    adapters: Vec<Box<dyn ExternalAdapter>>,
    sentiment_classifier: Option<Box<dyn SentimentClassifier>>,
    cache: crate::search_cache::SearchCache,
}

fn format_error_chain(error: &anyhow::Error) -> String {
    let parts = error
        .chain()
        .map(|cause| cause.to_string())
        .filter(|message| !message.trim().is_empty())
        .fold(Vec::<String>::new(), |mut acc, message| {
            if acc.last() != Some(&message) {
                acc.push(message);
            }
            acc
        });

    if parts.is_empty() {
        "unknown sample evaluation error".into()
    } else {
        parts.join(" | caused by: ")
    }
}

fn adapter_matches_filters(adapter: &dyn ExternalAdapter, filters: &SearchFilters) -> bool {
    if !filters.skill_ids.is_empty()
        && !filters
            .skill_ids
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(adapter.skill_id()))
    {
        return false;
    }

    if !filters.source_families.is_empty()
        && !filters
            .source_families
            .iter()
            .any(|candidate| *candidate == adapter.source_family())
    {
        return false;
    }

    let capabilities = adapter.capabilities();
    filters
        .required_capabilities
        .iter()
        .all(|required| capabilities.contains(required))
}

fn enrich_search_result_with_skill_metadata(
    mut result: SearchResult,
    adapter: &dyn ExternalAdapter,
) -> SearchResult {
    let mut attrs = result
        .source_attributes
        .take()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    attrs.insert(
        "skill_id".into(),
        serde_json::Value::String(adapter.skill_id().to_string()),
    );
    attrs.insert(
        "capabilities".into(),
        serde_json::to_value(adapter.capabilities()).unwrap_or_else(|_| serde_json::json!([])),
    );
    result.source_attributes = Some(serde_json::Value::Object(attrs));

    if result.provider_meta.is_none() {
        result.provider_meta = Some(data_core::types::ProviderMeta {
            provider_id: adapter.skill_id().to_string(),
            source_family: adapter.source_family(),
            labels: adapter.labels(),
        });
    }

    result
}

fn result_skill_id(result: &SearchResult) -> Option<String> {
    result
        .source_attributes
        .as_ref()
        .and_then(|value| value.get("skill_id"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .or_else(|| {
            result
                .provider_meta
                .as_ref()
                .map(|meta| meta.provider_id.clone())
        })
}

fn result_capabilities(result: &SearchResult) -> Vec<SkillCapability> {
    result
        .source_attributes
        .as_ref()
        .and_then(|value| value.get("capabilities"))
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn result_source_family(result: &SearchResult) -> Option<SourceFamily> {
    result.provider_meta.as_ref().map(|meta| meta.source_family)
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
            sentiment_classifier: None,
            cache: crate::search_cache::SearchCache::new(),
        }
    }

    pub fn with_cache(
        vector_index: VectorIndex,
        intent_parser: IntentParser,
        adapters: Vec<Box<dyn ExternalAdapter>>,
        cache: crate::search_cache::SearchCache,
    ) -> Self {
        Self {
            vector_index,
            intent_parser,
            adapters,
            sentiment_classifier: None,
            cache,
        }
    }

    /// Set the sentiment classifier used for on-chain review analysis.
    pub fn with_sentiment_classifier(mut self, classifier: Box<dyn SentimentClassifier>) -> Self {
        self.sentiment_classifier = Some(classifier);
        self
    }

    /// Returns all adapters that support the Subscribe capability.
    pub fn stream_adapters(&self) -> Vec<&dyn ExternalAdapter> {
        self.adapters
            .iter()
            .filter(|adapter| adapter.capabilities().contains(&SkillCapability::Subscribe))
            .map(|adapter| adapter.as_ref())
            .collect()
    }

    fn adapter_by_skill_id(&self, skill_id: &str) -> Option<&dyn ExternalAdapter> {
        self.adapters
            .iter()
            .find(|adapter| adapter.skill_id().eq_ignore_ascii_case(skill_id))
            .map(|adapter| adapter.as_ref())
    }

    pub async fn lookup_by_skill(
        &self,
        skill_id: &str,
        id: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let adapter = self
            .adapter_by_skill_id(skill_id)
            .with_context(|| format!("adapter not found for skill: {skill_id}"))?;
        adapter.lookup(id).await
    }

    pub async fn download_by_skill(
        &self,
        skill_id: &str,
        id: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let adapter = self
            .adapter_by_skill_id(skill_id)
            .with_context(|| format!("adapter not found for skill: {skill_id}"))?;
        adapter.download(id).await
    }

    pub async fn schema_probe_by_skill(
        &self,
        skill_id: &str,
        id: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let adapter = self
            .adapter_by_skill_id(skill_id)
            .with_context(|| format!("adapter not found for skill: {skill_id}"))?;
        adapter.schema_probe(id).await
    }

    pub async fn query_by_skill(
        &self,
        skill_id: &str,
        id: &str,
        question: &str,
    ) -> Result<serde_json::Value> {
        let adapter = self
            .adapter_by_skill_id(skill_id)
            .with_context(|| format!("adapter not found for skill: {skill_id}"))?;
        adapter.query(id, question).await
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
        // Generate cache key from query and filters
        let filters_hash = {
            let mut hasher = sha2::Sha256::new();
            use sha2::Digest;
            hasher.update(
                serde_json::to_string(filters)
                    .unwrap_or_default()
                    .as_bytes(),
            );
            hex::encode(hasher.finalize())
        };
        let cache_key = format!("search:v1:{}:{}", query.trim().to_lowercase(), filters_hash);

        // Check cache
        if let Some(cached_results) = self.cache.get(&cache_key).await {
            tracing::debug!(query = %query, "search cache hit");
            let ranked: Vec<RankedResult> = cached_results
                .into_iter()
                .map(|r| RankedResult {
                    signal: signal_fetcher(&r.cid.0),
                    result: r,
                    rank_score: 0.0,
                })
                .collect();
            return Ok(SearchOutput {
                results: ranked,
                errors: vec![],
                profile: None,
            });
        }

        // Cache miss — execute search
        let output = self
            .search_with_task_type(query, None, filters, local_metadata, signal_fetcher, limit)
            .await?;

        // Store results in cache
        let results_to_cache: Vec<SearchResult> =
            output.results.iter().map(|r| r.result.clone()).collect();
        let ttl = crate::search_cache::SearchCache::ttl_for_skill("default");
        self.cache.set(cache_key, results_to_cache, ttl).await;

        Ok(output)
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

        let normalized_keywords = normalized_intent_keywords(profile);
        let task_text = profile
            .task_description
            .as_deref()
            .filter(|description| !description.trim().is_empty())
            .unwrap_or(profile.raw_query.as_str());
        let local_metadata_by_cid: HashMap<&str, &DatasetMetadata> = local_metadata
            .iter()
            .map(|metadata| (metadata.cid.0.as_str(), metadata))
            .collect();
        let mut all: Vec<SearchResult> = local_results.unwrap_or_default();
        all.extend(external_results);

        // Apply filters
        if let Some(ref topic) = filters.topic {
            let topic = topic.to_lowercase();
            all.retain(|r| normalized_search_result_text(r).contains(&topic));
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
        if !filters.skill_ids.is_empty() {
            all.retain(|r| {
                result_skill_id(r).is_some_and(|skill_id| {
                    filters
                        .skill_ids
                        .iter()
                        .any(|candidate| candidate.eq_ignore_ascii_case(&skill_id))
                })
            });
        }
        if !filters.source_families.is_empty() {
            all.retain(|r| {
                result_source_family(r)
                    .is_some_and(|family| filters.source_families.contains(&family))
            });
        }
        if !filters.required_capabilities.is_empty() {
            all.retain(|r| {
                let capabilities = result_capabilities(r);
                filters
                    .required_capabilities
                    .iter()
                    .all(|required| capabilities.contains(required))
            });
        }

        apply_intent_hard_filters(&mut all, profile, &local_metadata_by_cid, filters);

        // Deduplicate by CID, preferring results with richer source_attributes
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut deduped: Vec<SearchResult> = Vec::with_capacity(all.len());
        for r in all {
            if let Some(&existing_idx) = seen.get(&r.cid.0) {
                // Replace if the new result has source_attributes and the existing one doesn't
                let existing_attrs_len = deduped[existing_idx]
                    .source_attributes
                    .as_ref()
                    .map(|v| v.as_object().map_or(0, |o| o.len()))
                    .unwrap_or(0);
                let new_attrs_len = r
                    .source_attributes
                    .as_ref()
                    .map(|v| v.as_object().map_or(0, |o| o.len()))
                    .unwrap_or(0);
                if new_attrs_len > existing_attrs_len {
                    deduped[existing_idx] = r;
                }
            } else {
                seen.insert(r.cid.0.clone(), deduped.len());
                deduped.push(r);
            }
        }
        let all = deduped;

        // Rank with community signal (TCV-lite for search ranking)
        let signal_by_cid: HashMap<String, CommunitySignal> = all
            .iter()
            .map(|result| (result.cid.0.clone(), signal_fetcher(&result.cid.0)))
            .collect();
        let mut ranked: Vec<RankedResult> = all
            .into_iter()
            .map(|r| {
                let metadata = local_metadata_by_cid.get(r.cid.0.as_str()).copied();
                let signal = signal_by_cid
                    .get(&r.cid.0)
                    .cloned()
                    .unwrap_or_else(|| empty_signal_for_cid(&r.cid));
                let score = rank_with_signal(
                    &r,
                    metadata,
                    &signal,
                    task_text,
                    &normalized_keywords,
                    profile,
                );
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

    /// Search with a caller-provided structured profile while pinning execution
    /// to a single external skill.
    pub async fn search_single_skill_with_profile(
        &self,
        profile: &QueryProfile,
        skill_id: &str,
        filters: &SearchFilters,
        local_metadata: &[DatasetMetadata],
        signal_fetcher: &SignalFetcher,
        limit: usize,
    ) -> Result<SearchOutput> {
        let mut scoped_filters = filters.clone();
        scoped_filters.skill_ids = vec![skill_id.to_string()];
        self.search_with_profile(
            profile,
            &scoped_filters,
            local_metadata,
            signal_fetcher,
            limit,
        )
        .await
    }

    /// Search one external skill directly, mirroring the adapter-driven
    /// platform query flow used in the upstream Guixu repository.
    pub async fn search_single_skill_raw(
        &self,
        skill_id: &str,
        query: &str,
        signal_fetcher: &SignalFetcher,
        limit: usize,
    ) -> Result<Vec<RankedResult>> {
        let adapter = self
            .adapter_by_skill_id(skill_id)
            .with_context(|| format!("adapter not found for skill: {skill_id}"))?;
        let results = adapter.search(query, limit).await?;

        Ok(results
            .into_iter()
            .take(limit)
            .map(|result| {
                let result = enrich_search_result_with_skill_metadata(result, adapter);
                RankedResult {
                    signal: signal_fetcher(&result.cid.0),
                    result,
                    rank_score: 0.0,
                }
            })
            .collect())
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
        request: SearchAndValueRequest<'_>,
    ) -> Result<DatasetSelectionOutput> {
        let profile = self.intent_parser.profile(request.query).await?;
        let task = request
            .task
            .cloned()
            .unwrap_or_else(|| DatasetSelectionTask::from(&profile));
        let search_output = self
            .search_with_profile(
                &profile,
                request.filters,
                request.local_metadata,
                request.signal_fetcher,
                request.limit,
            )
            .await?;

        self.value_search_output(
            &search_output,
            request.local_metadata,
            request.metadata_resolver,
            request.sample_evaluator,
            &task,
            request.config,
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
        let mut errors = search_output.errors.clone();
        let local_metadata_by_cid: HashMap<&str, &DatasetMetadata> = local_metadata
            .iter()
            .map(|metadata| (metadata.cid.0.as_str(), metadata))
            .collect();

        let mut candidates = Vec::with_capacity(search_output.results.len());

        let scoring_profile = detect_scoring_profile(task, &search_output.results);

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

            let relevance = compute_relevance(task, &ranked.result, metadata.as_ref());
            let on_chain_score =
                match compute_on_chain_score(&ranked.result, self.sentiment_classifier.as_deref())
                    .await
                {
                    Ok(score) => score,
                    Err(error) => {
                        warn!(
                            cid = %ranked.result.cid.0,
                            error = %error,
                            "on-chain scoring failed; falling back to neutral"
                        );
                        ON_CHAIN_SCORE_NEUTRAL
                    }
                };

            let (coarse_score, schema_fit, scale_score, label_quality, metadata_completeness) =
                match scoring_profile {
                    ScoringProfile::Academic => {
                        let citation = compute_citation_score(&ranked.result);
                        let venue = compute_venue_score(&ranked.result);
                        let recency = compute_academic_recency(&ranked.result);
                        let abstract_q = compute_abstract_quality(&ranked.result);
                        let score = compose_academic_score(
                            relevance,
                            citation,
                            venue,
                            recency,
                            abstract_q,
                            on_chain_score,
                        );
                        // Map academic dimensions into the existing struct fields
                        // schema_fit → citation, scale_score → venue, label_quality → recency
                        (score, citation, venue, recency, abstract_q)
                    }
                    ScoringProfile::Dataset => {
                        let sf = compute_schema_fit(task, &ranked.result, metadata.as_ref());
                        let sc = compute_scale_score(config, &ranked.result, metadata.as_ref());
                        let ba = compute_label_quality(task, &ranked.result, metadata.as_ref());
                        let mq = compute_metadata_completeness(&ranked.result, metadata.as_ref());
                        let score = compose_coarse_score(
                            relevance,
                            sf,
                            sc,
                            ba,
                            mq,
                            on_chain_score,
                            compute_freshness_bonus(&ranked.result),
                        );
                        (score, sf, sc, ba, mq)
                    }
                };

            candidates.push(ValuationCandidateState {
                metadata: metadata.clone(),
                valuation: DatasetValuation {
                    result: ranked.result.clone(),
                    coarse_score,
                    final_score: coarse_score,
                    relevance,
                    schema_fit,
                    scale_score,
                    label_quality,
                    metadata_completeness,
                    on_chain_score,
                    metadata_resolved: metadata.is_some(),
                    sample_plan: None,
                    proxy_utility: None,
                    sample_failure_reason: None,
                    explanation: String::new(),
                },
            });
        }

        candidates.sort_by(|a, b| {
            b.valuation
                .coarse_score
                .partial_cmp(&a.valuation.coarse_score)
                .unwrap_or(Ordering::Equal)
        });

        let batch_size = config.coarse_top_k.max(1);
        let mut selected_collection_plan = None;
        let mut evaluated_batch_count = 0usize;
        let mut stop_after_batch_end = None;

        for (batch_index, batch_start) in (0..candidates.len()).step_by(batch_size).enumerate() {
            let batch_end = (batch_start + batch_size).min(candidates.len());
            evaluated_batch_count += 1;

            let Some(sample_evaluator) = sample_evaluator else {
                // Mark all candidates in this batch as skipped (no evaluator available)
                for candidate in candidates.iter_mut().take(batch_end).skip(batch_start) {
                    candidate.valuation.sample_failure_reason =
                        Some("sample evaluation skipped: evaluator unavailable".into());
                }
                continue;
            };

            // First pass: clone data needed for evaluation and identify candidates to evaluate
            #[derive(Clone)]
            struct EvalTask {
                result: SearchResult,
                metadata: DatasetMetadata,
                plan: SamplePlan,
            }

            let mut eval_tasks: Vec<EvalTask> = Vec::new();
            let mut candidates_to_evaluate: Vec<usize> = Vec::new();

            for (i, candidate) in candidates
                .iter_mut()
                .enumerate()
                .take(batch_end)
                .skip(batch_start)
            {
                let Some(metadata) = candidate.metadata.clone() else {
                    candidate.valuation.sample_failure_reason =
                        Some("sample evaluation skipped: resolved metadata unavailable".into());
                    errors.push(format!(
                        "sample evaluation skipped for {}: metadata unavailable",
                        candidate.valuation.result.cid.0
                    ));
                    continue;
                };

                let plan = build_sample_plan(&metadata, config);
                if plan.sample_fraction == 0.0
                    || (plan.estimated_rows == 0
                        && !supports_sampling_without_shape_estimate(&candidate.valuation.result))
                {
                    candidate.valuation.sample_failure_reason = Some(
                        if plan.sample_fraction == 0.0 {
                            "sample evaluation skipped: sampling plan resolved to zero sample fraction"
                                .into()
                        } else {
                            "sample evaluation skipped: dataset shape unavailable for sample download"
                                .into()
                        },
                    );
                    continue;
                }

                candidate.valuation.sample_plan = Some(plan.clone());
                candidates_to_evaluate.push(i);
                eval_tasks.push(EvalTask {
                    result: candidate.valuation.result.clone(),
                    metadata,
                    plan,
                });
            }

            // Launch all evaluations in parallel using join_all
            let task_ref = task;
            let futures: Vec<_> = eval_tasks
                .into_iter()
                .map(|eval_task| {
                    let evaluator = sample_evaluator;
                    async move {
                        evaluator
                            .evaluate_sample(
                                &eval_task.result,
                                &eval_task.metadata,
                                task_ref,
                                &eval_task.plan,
                            )
                            .await
                    }
                })
                .collect();

            let outcomes = join_all(futures).await;

            // Pair results with indices
            let results: Vec<(usize, Result<SampleEvaluationOutcome>)> = outcomes
                .into_iter()
                .zip(candidates_to_evaluate.into_iter())
                .map(|(outcome, i)| (i, outcome))
                .collect();

            // Second pass: update candidates with evaluation results
            for (i, outcome_result) in results {
                let candidate = &mut candidates[i];
                match outcome_result {
                    Ok(outcome) => {
                        if let Some(proxy_utility) = outcome.proxy_utility {
                            candidate.valuation.sample_failure_reason = None;
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
                        } else {
                            candidate.valuation.sample_failure_reason =
                                Some(outcome.no_score_reason.unwrap_or_else(|| {
                                    "sample evaluation returned no usable score".into()
                                }));
                        }
                    }
                    Err(error) => {
                        let formatted = format_error_chain(&error);
                        candidate.valuation.sample_failure_reason = Some(formatted.clone());
                        errors.push(format!(
                            "sample evaluation failed for {}: {}",
                            candidate.valuation.result.cid.0, formatted
                        ));
                    }
                }
            }

            if let Some(plan) =
                knapsack_select_from_batch(task, &candidates[..batch_end], 0, batch_index)
            {
                stop_after_batch_end = Some(batch_end);
                selected_collection_plan = Some(plan);
                break;
            }
        }

        if let Some(skipped_start) = stop_after_batch_end {
            for candidate in candidates.iter_mut().skip(skipped_start) {
                if candidate.valuation.sample_failure_reason.is_none() && sample_evaluator.is_some()
                {
                    candidate.valuation.sample_failure_reason = Some(
                        "sample evaluation skipped: collection selection succeeded in an earlier batch"
                            .into(),
                    );
                }
            }
        }

        if selected_collection_plan.is_none() && task.has_collection_constraints() {
            errors.push(format!(
                "dataset collection selection failed after {} batch(es): budget <= {} and total size {}",
                evaluated_batch_count,
                task.max_total_price
                    .map(|value| format!("{value:.4}"))
                    .unwrap_or_else(|| "unbounded".into()),
                describe_size_interval(task.min_total_size_bytes, task.max_total_size_bytes),
            ));
        }

        for candidate in &mut candidates {
            candidate.valuation.explanation = build_valuation_explanation(&candidate.valuation);
        }

        let selected_collection = selected_collection_plan
            .as_ref()
            .map(|plan| build_selected_collection(plan, &candidates));

        candidates.sort_by(|a, b| {
            b.valuation
                .final_score
                .partial_cmp(&a.valuation.final_score)
                .unwrap_or(Ordering::Equal)
        });

        let ranked_candidates: Vec<DatasetValuation> = candidates
            .into_iter()
            .map(|candidate| candidate.valuation)
            .collect();
        let selected = selected_collection
            .as_ref()
            .and_then(select_representative_dataset)
            .or_else(|| ranked_candidates.first().cloned());

        Ok(DatasetSelectionOutput {
            task: task.clone(),
            selected,
            selected_collection,
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
                let all_text = normalized_metadata_text(m);

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

    /// Search external platforms via adapters (concurrently).
    async fn search_external(
        &self,
        intent: &ParsedIntent,
        filters: &SearchFilters,
        limit: usize,
    ) -> (Vec<SearchResult>, Vec<String>) {
        let external_query = build_search_query(intent);
        let timeout_duration = Duration::from_secs(ADAPTER_SEARCH_TIMEOUT_SECS);
        let mut futures: FuturesUnordered<_> = self
            .adapters
            .iter()
            .filter(|a| adapter_matches_filters(a.as_ref(), filters))
            .map(|adapter| {
                let query = external_query.clone();
                async move {
                    let result =
                        tokio::time::timeout(timeout_duration, adapter.search(&query, limit)).await;
                    (adapter.name().to_string(), result)
                }
            })
            .collect();

        let mut results = vec![];
        let mut errors = vec![];
        while let Some((name, outcome)) = futures.next().await {
            match outcome {
                Ok(Ok(r)) => {
                    if let Some(adapter) = self.adapters.iter().find(|a| a.name() == name.as_str())
                    {
                        results.extend(r.into_iter().map(|result| {
                            enrich_search_result_with_skill_metadata(result, adapter.as_ref())
                        }));
                    } else {
                        results.extend(r);
                    }
                }
                Ok(Err(e)) => {
                    warn!(adapter = %name, error = %e, "adapter search failed");
                    errors.push(format!("{name}: {e}"));
                }
                Err(_) => {
                    warn!(adapter = %name, timeout_secs = %ADAPTER_SEARCH_TIMEOUT_SECS, "adapter search timed out");
                    errors.push(format!(
                        "{name}: timeout after {}s",
                        ADAPTER_SEARCH_TIMEOUT_SECS
                    ));
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

#[derive(Debug, Clone)]
struct ValuationCandidateState {
    valuation: DatasetValuation,
    metadata: Option<DatasetMetadata>,
}

/// Structured description of the downstream training task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSelectionTask {
    pub task_description: String,
    pub task_type: String,
    pub required_columns: Vec<String>,
    pub target_entity: Option<String>,
    pub required_data_type: Option<DataType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_price: Option<f64>,
    #[serde(default)]
    pub min_total_size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_size_bytes: Option<u64>,
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
            max_total_price: parse_budget_amount_str(&profile.data_standard.budget),
            min_total_size_bytes: profile.data_standard.min_dataset_size_bytes,
            max_total_size_bytes: Some(profile.data_standard.max_dataset_size_bytes)
                .filter(|value| *value > 0),
        }
    }
}

impl DatasetSelectionTask {
    pub fn has_collection_constraints(&self) -> bool {
        self.max_total_price.is_some()
            || self.min_total_size_bytes > 0
            || self.max_total_size_bytes.is_some()
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

/// Outcome of a sample-evaluation attempt, including an optional no-score reason.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleEvaluationOutcome {
    pub proxy_utility: Option<ProxyUtilityReport>,
    pub no_score_reason: Option<String>,
}

impl SampleEvaluationOutcome {
    pub fn scored(proxy_utility: ProxyUtilityReport) -> Self {
        Self {
            proxy_utility: Some(proxy_utility),
            no_score_reason: None,
        }
    }

    pub fn no_score(reason: impl Into<String>) -> Self {
        Self {
            proxy_utility: None,
            no_score_reason: Some(reason.into()),
        }
    }
}

/// Full valuation record for a candidate dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetValuation {
    pub result: SearchResult,
    pub coarse_score: f64,
    pub final_score: f64,
    pub relevance: f64,
    pub schema_fit: f64,
    pub scale_score: f64,
    pub label_quality: f64,
    pub metadata_completeness: f64,
    pub on_chain_score: f64,
    pub metadata_resolved: bool,
    pub sample_plan: Option<SamplePlan>,
    pub proxy_utility: Option<ProxyUtilityReport>,
    pub sample_failure_reason: Option<String>,
    pub explanation: String,
}

#[derive(Debug, Clone, Default)]
struct OnChainScoreInputs {
    trade_count: u64,
    avg_rating: f64,
    positive_reviews: u64,
    neutral_reviews: u64,
    negative_reviews: u64,
}

impl OnChainScoreInputs {
    fn review_count(&self) -> u64 {
        self.positive_reviews + self.neutral_reviews + self.negative_reviews
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewSentiment {
    Positive,
    Neutral,
    Negative,
}

/// End-to-end output for search + valuation + best-dataset selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSelectionOutput {
    pub task: DatasetSelectionTask,
    pub selected: Option<DatasetValuation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_collection: Option<DatasetCollectionSelection>,
    pub candidates: Vec<DatasetValuation>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetCollectionSelection {
    pub datasets: Vec<DatasetValuation>,
    pub total_price: f64,
    pub total_size_bytes: u64,
    pub total_value: f64,
    pub batch_index: usize,
    pub batch_start_rank: usize,
    pub batch_end_rank: usize,
    pub price_step: f64,
}

#[derive(Debug, Clone)]
struct DatasetCollectionSelectionPlan {
    candidate_indices: Vec<usize>,
    total_price: f64,
    total_size_bytes: u64,
    total_value: f64,
    batch_index: usize,
    batch_start_rank: usize,
    batch_end_rank: usize,
    price_step: f64,
}

#[derive(Debug, Clone)]
struct SelectionBatchCandidate {
    global_index: usize,
    price_amount: f64,
    price_units: usize,
    size_bytes: u64,
    total_value: f64,
}

#[derive(Debug, Clone, Default)]
struct CollectionDpState {
    total_price: f64,
    total_size_bytes: u64,
    total_value: f64,
    chosen_indices: Vec<usize>,
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
    ) -> Result<SampleEvaluationOutcome>;
}

/// Backward-compatible alias for the structured query profile type.
pub type ParsedIntent = QueryProfile;

/// Rank a search result incorporating community signal.
/// This is a lightweight version of TCV used for search ranking.
fn rank_with_signal(
    result: &SearchResult,
    metadata: Option<&DatasetMetadata>,
    signal: &CommunitySignal,
    task_text: &str,
    keywords: &[String],
    intent: &ParsedIntent,
) -> f64 {
    let quality = result.quality.as_ref().map(|q| q.total).unwrap_or(50.0);

    let metadata_text = normalized_dataset_text(result, metadata);
    let searchable_text = normalized_search_result_text(result);
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

    // Academic results get citation/venue boost instead of generic quality/community
    if is_academic_result(result) {
        let citation = compute_citation_score(result);
        let venue = compute_venue_score(result);
        let recency = compute_academic_recency(result);
        return (0.40 * relevance
            + 0.25 * citation
            + 0.15 * venue
            + 0.10 * recency
            + 0.05 * community
            + 0.05 * market_boost
            - price_penalty)
            .clamp(0.0, 100.0);
    }

    // Weighted combination (default dataset profile)
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

fn empty_signal_for_cid(cid: &DatasetCid) -> CommunitySignal {
    CommunitySignal {
        dataset_cid: cid.clone(),
        total_reviews: 0,
        avg_relevance: 0.0,
        avg_quality: 0.0,
        positive_rate: 0.0,
        negative_rate: 0.0,
        task_signals: vec![],
    }
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
    _metadata_by_cid: &HashMap<&str, &DatasetMetadata>,
    filters: &SearchFilters,
) {
    if let Some(required_type) = required_data_type(profile) {
        results.retain(|result| result.data_type == required_type);
    }
    if let Some(ref chain) = filters.chain {
        results.retain(|r| attr_matches(&r.source_attributes, "chain", chain));
    }
    if let Some(ref protocol) = filters.protocol {
        results.retain(|r| attr_matches(&r.source_attributes, "protocol", protocol));
    }
    if let Some(ref category) = filters.category {
        results.retain(|r| attr_matches(&r.source_attributes, "category", category));
    }
    if Some(true) == filters.free_only {
        results.retain(|r| r.price.is_free());
    }
}

fn attr_matches(attrs: &Option<serde_json::Value>, key: &str, expected: &str) -> bool {
    attrs
        .as_ref()
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_str())
        .map(|v| v.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
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

pub(crate) fn compose_coarse_score(
    relevance: f64,
    schema_fit: f64,
    scale_score: f64,
    label_quality: f64,
    metadata_completeness: f64,
    on_chain_score: f64,
    freshness_bonus: f64,
) -> f64 {
    (0.50 * relevance
        + 0.15 * schema_fit
        + 0.15 * scale_score
        + 0.10 * label_quality
        + 0.05 * metadata_completeness
        + ON_CHAIN_COARSE_ADJUST_WEIGHT * (on_chain_score - ON_CHAIN_SCORE_NEUTRAL)
        + 0.05 * freshness_bonus)
        .clamp(0.0, 100.0)
}

// ── Scoring profile auto-detection ──────────────────────────────────────

/// Scoring profiles for different data discovery scenarios.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScoringProfile {
    /// Default: tabular/structured dataset evaluation.
    Dataset,
    /// Academic literature discovery (papers from DBLP, S2, arXiv, etc.).
    Academic,
}

/// Detect the appropriate scoring profile from the task and candidate results.
pub(crate) fn detect_scoring_profile(
    task: &DatasetSelectionTask,
    results: &[RankedResult],
) -> ScoringProfile {
    // Check task type hints
    let task_type = task.task_type.to_lowercase();
    if task_type == "retrieval" || task_type == "literature_review" || task_type == "survey" {
        let desc = task.task_description.to_lowercase();
        if desc.contains("paper")
            || desc.contains("literature")
            || desc.contains("survey")
            || desc.contains("academic")
            || desc.contains("publication")
            || desc.contains("文献")
            || desc.contains("论文")
            || desc.contains("调研")
        {
            return ScoringProfile::Academic;
        }
    }

    // Check if majority of results are academic
    if results.len() >= 3 {
        let academic_count = results
            .iter()
            .filter(|r| is_academic_result(&r.result))
            .count();
        if academic_count * 2 >= results.len() {
            return ScoringProfile::Academic;
        }
    }

    ScoringProfile::Dataset
}

/// Check if a search result is from an academic source.
fn is_academic_result(result: &SearchResult) -> bool {
    matches!(
        result.source,
        DataSource::Dblp | DataSource::SemanticScholar | DataSource::Arxiv
    ) || result
        .source_attributes
        .as_ref()
        .and_then(|v| v.get("academic"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    || result.cid.0.contains("arxiv.org")
    || result.cid.0.starts_with("10.")  // DOI
    || result.cid.0.starts_with("s2:")
    || result.cid.0.starts_with("dblp:")
}

/// Extract citation count from source_attributes, falling back to description text.
fn extract_citation_count(result: &SearchResult) -> u64 {
    // Primary: source_attributes
    if let Some(count) = result
        .source_attributes
        .as_ref()
        .and_then(|v| v.get("citation_count"))
        .and_then(|v| v.as_u64())
    {
        return count;
    }
    // Fallback: parse "Citations: N" from description (Semantic Scholar format)
    if let Some(desc) = &result.description {
        if let Some(pos) = desc.find("Citations: ") {
            let after = &desc[pos + 11..];
            let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_str.parse::<u64>() {
                return n;
            }
        }
    }
    0
}

/// Compute citation impact score (log-saturating, 0-100).
fn compute_citation_score(result: &SearchResult) -> f64 {
    let citations = extract_citation_count(result);
    if citations == 0 {
        return 20.0; // neutral baseline for uncited/unknown
    }
    // log saturation: 100 citations → ~80, 1000 → ~95
    let score = (citations as f64).ln() / (500.0_f64).ln() * 80.0 + 20.0;
    score.clamp(0.0, 100.0)
}

/// Compute venue quality score based on known venue tiers.
fn compute_venue_score(result: &SearchResult) -> f64 {
    // Primary: source_attributes
    let venue = result
        .source_attributes
        .as_ref()
        .and_then(|v| v.get("venue"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !venue.is_empty() {
        return score_venue_name(venue);
    }
    // Fallback: tags
    if let Some(tag) = result.tags.first() {
        if !tag.is_empty() {
            return score_venue_name(tag);
        }
    }
    // Fallback: parse venue from DBLP-style description "Author (Year). Venue."
    if let Some(desc) = &result.description {
        if let Some(pos) = desc.find("). ") {
            let after = &desc[pos + 3..];
            let venue_part: String = after
                .chars()
                .take_while(|&c| c != '.' && c != '\n')
                .collect();
            if !venue_part.is_empty() && venue_part.len() < 80 {
                return score_venue_name(&venue_part);
            }
        }
    }
    30.0
}

fn score_venue_name(venue: &str) -> f64 {
    let v = venue.to_lowercase();
    // Top-tier venues
    if [
        "neurips", "nips", "icml", "iclr", "aaai", "ijcai", "cvpr", "iccv", "eccv", "acl", "emnlp",
        "naacl", "sigmod", "vldb", "icde", "kdd", "www", "sigir", "nature", "science", "cell",
        "pnas", "jmlr", "tmlr",
    ]
    .iter()
    .any(|t| v.contains(t))
    {
        return 95.0;
    }
    // Strong venues
    if ["corr", "arxiv", "transactions", "journal", "ieee", "acm"]
        .iter()
        .any(|t| v.contains(t))
    {
        return 60.0;
    }
    // Conference/workshop
    if v.contains("conference") || v.contains("workshop") || v.contains("proceedings") {
        return 50.0;
    }
    40.0
}

/// Compute recency score for academic papers (newer = better for survey tasks).
fn compute_academic_recency(result: &SearchResult) -> f64 {
    let age_days = (chrono::Utc::now() - result.created_at).num_days().max(0) as f64;
    if age_days < 180.0 {
        100.0 // < 6 months
    } else if age_days < 365.0 {
        85.0 // < 1 year
    } else if age_days < 730.0 {
        70.0 // < 2 years
    } else if age_days < 1825.0 {
        50.0 // < 5 years
    } else {
        30.0
    }
}

/// Compute abstract/description quality for academic results.
fn compute_abstract_quality(result: &SearchResult) -> f64 {
    let desc_len = result.description.as_ref().map(|d| d.len()).unwrap_or(0);
    let has_pdf = result
        .source_attributes
        .as_ref()
        .and_then(|v| v.get("has_pdf"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut score: f64 = match desc_len {
        0 => 10.0,
        1..=50 => 30.0,
        51..=200 => 60.0,
        _ => 80.0,
    };
    if has_pdf {
        score += 20.0;
    }
    score.clamp(0.0, 100.0)
}

/// Academic scoring: replaces schema_fit/scale_score/label_quality with
/// citation_impact/venue_quality/recency for literature discovery.
pub(crate) fn compose_academic_score(
    relevance: f64,
    citation_score: f64,
    venue_score: f64,
    recency_score: f64,
    abstract_quality: f64,
    on_chain_score: f64,
) -> f64 {
    (0.40 * relevance
        + 0.25 * citation_score
        + 0.15 * venue_score
        + 0.10 * recency_score
        + 0.05 * abstract_quality
        + 0.05 * (on_chain_score - ON_CHAIN_SCORE_NEUTRAL + 50.0))
        .clamp(0.0, 100.0)
}

fn compute_freshness_bonus(result: &SearchResult) -> f64 {
    let cadence = result
        .source_attributes
        .as_ref()
        .and_then(|v| v.get("refresh_cadence"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if cadence.is_empty() {
        return 0.0;
    }
    let age_hours = (chrono::Utc::now() - result.created_at).num_hours().max(0) as f64;
    match cadence {
        "daily" if age_hours < 24.0 => 100.0,
        "daily" if age_hours < 168.0 => 70.0,
        "weekly" if age_hours < 336.0 => 50.0,
        _ => 0.0,
    }
}

async fn compute_on_chain_score(
    result: &SearchResult,
    classifier: Option<&dyn SentimentClassifier>,
) -> Result<f64> {
    if result.source != DataSource::GuixuHub {
        return Ok(ON_CHAIN_SCORE_NEUTRAL);
    }

    let trade_count = result
        .market
        .as_ref()
        .map(|market| market.trade_count)
        .unwrap_or(0);
    let mut inputs = OnChainScoreInputs {
        trade_count,
        ..Default::default()
    };

    let Some(listing_id) = guixu_listing_id(result) else {
        return Ok(score_on_chain_signal(&inputs));
    };

    match fetch_guixu_review_inputs(listing_id, classifier).await {
        Ok(review_inputs) => {
            inputs.avg_rating = review_inputs.avg_rating;
            inputs.positive_reviews = review_inputs.positive_reviews;
            inputs.neutral_reviews = review_inputs.neutral_reviews;
            inputs.negative_reviews = review_inputs.negative_reviews;
        }
        Err(error) => {
            warn!(
                cid = %result.cid.0,
                listing_id,
                error = %error,
                "guixu review fetch failed; using trade-only on-chain signal"
            );
        }
    }

    Ok(score_on_chain_signal(&inputs))
}

fn guixu_listing_id(result: &SearchResult) -> Option<&str> {
    result
        .cid
        .0
        .strip_prefix("guixu-hub:")
        .or_else(|| result.provider.0.strip_prefix("guixu:hub:"))
        .or_else(|| result.provider.0.strip_prefix("guixu.market:"))
}

async fn fetch_guixu_review_inputs(
    listing_id: &str,
    classifier: Option<&dyn SentimentClassifier>,
) -> Result<OnChainScoreInputs> {
    let Some(contract_address) = env_or_setting("GUIXU_BASE_CONTRACT_ADDRESS")
        .or_else(|| env_or_setting("GUIXU_CONTRACT_ADDRESS"))
    else {
        return Ok(OnChainScoreInputs::default());
    };

    let network = env_or_setting("GUIXU_BASE_NETWORK").unwrap_or_else(|| "mainnet".to_string());
    let config = match network.trim().to_lowercase().as_str() {
        "mainnet" | "base" => ChainConfig::base_mainnet(&contract_address),
        "sepolia" | "base-sepolia" => ChainConfig::base_sepolia(&contract_address),
        other => {
            warn!(
                network = other,
                "unsupported GUIXU_BASE_NETWORK; falling back to Base mainnet"
            );
            ChainConfig::base_mainnet(&contract_address)
        }
    };

    let client = BaseChainClient::new(config);
    let reviews = fetch_buyer_reviews(&client, listing_id)
        .await
        .with_context(|| format!("fetch buyer reviews for listing {listing_id}"))?;

    if reviews.is_empty() {
        return Ok(OnChainScoreInputs::default());
    }

    let summary = summarize_reviews(listing_id, &reviews);
    let classified = classify_review_sentiments(&reviews, classifier).await;
    Ok(OnChainScoreInputs {
        trade_count: 0,
        avg_rating: summary.avg_rating,
        positive_reviews: classified.positive_reviews,
        neutral_reviews: classified.neutral_reviews,
        negative_reviews: classified.negative_reviews,
    })
}

#[derive(Debug, Clone, Default)]
struct ClassifiedReviewCounts {
    positive_reviews: u64,
    neutral_reviews: u64,
    negative_reviews: u64,
}

impl ClassifiedReviewCounts {
    fn push(&mut self, sentiment: ReviewSentiment) {
        match sentiment {
            ReviewSentiment::Positive => self.positive_reviews += 1,
            ReviewSentiment::Neutral => self.neutral_reviews += 1,
            ReviewSentiment::Negative => self.negative_reviews += 1,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CommentClassificationItem {
    pub id: usize,
    pub rating: u8,
    pub comment: String,
}

#[derive(Debug, Deserialize)]
pub struct DeepSeekSentimentPayload {
    pub items: Vec<DeepSeekSentimentItem>,
}

#[derive(Debug, Deserialize)]
pub struct DeepSeekSentimentItem {
    pub id: usize,
    pub sentiment: String,
}

async fn classify_review_sentiments(
    reviews: &[data_attestation::BuyerReview],
    classifier: Option<&dyn SentimentClassifier>,
) -> ClassifiedReviewCounts {
    let mut fallback_counts = ClassifiedReviewCounts::default();
    let mut comment_items = Vec::new();
    let mut fallback_by_id = HashMap::new();

    for (id, review) in reviews.iter().enumerate() {
        let fallback = sentiment_from_rating(review.rating);
        let comment = review.comment.trim();
        if comment.is_empty() || comment_items.len() >= ON_CHAIN_SENTIMENT_SAMPLE_LIMIT {
            fallback_counts.push(fallback);
            continue;
        }

        fallback_by_id.insert(id, fallback);
        comment_items.push(CommentClassificationItem {
            id,
            rating: review.rating,
            comment: comment.to_string(),
        });
    }

    if comment_items.is_empty() {
        return fallback_counts;
    }

    let classified = if let Some(clf) = classifier {
        clf.classify(&comment_items).await
    } else {
        // No classifier available — use rating-based fallback only.
        Err(anyhow::anyhow!("no sentiment classifier configured"))
    };

    match classified {
        Ok(classified) => {
            for item in comment_items {
                let sentiment = classified
                    .get(&item.id)
                    .copied()
                    .unwrap_or_else(|| fallback_by_id[&item.id]);
                fallback_counts.push(sentiment);
            }
            fallback_counts
        }
        Err(error) => {
            warn!(error = %error, "sentiment classification unavailable; falling back to ratings");
            for item in comment_items {
                fallback_counts.push(fallback_by_id[&item.id]);
            }
            fallback_counts
        }
    }
}

fn sentiment_from_rating(rating: u8) -> ReviewSentiment {
    match rating {
        4..=5 => ReviewSentiment::Positive,
        0..=2 => ReviewSentiment::Negative,
        _ => ReviewSentiment::Neutral,
    }
}

pub fn parse_review_sentiment(raw: &str) -> Option<ReviewSentiment> {
    let normalized = raw.trim().to_lowercase();
    if normalized.is_empty() {
        return None;
    }

    if normalized.contains("positive") || normalized.contains("好评") || normalized.contains("满意")
    {
        Some(ReviewSentiment::Positive)
    } else if normalized.contains("negative")
        || normalized.contains("差评")
        || normalized.contains("不满")
    {
        Some(ReviewSentiment::Negative)
    } else if normalized.contains("neutral") || normalized.contains("中性") {
        Some(ReviewSentiment::Neutral)
    } else {
        None
    }
}

fn score_on_chain_signal(inputs: &OnChainScoreInputs) -> f64 {
    let review_count = inputs.review_count();
    if inputs.trade_count == 0 && review_count == 0 {
        return ON_CHAIN_SCORE_NEUTRAL;
    }

    let trade_score = if inputs.trade_count == 0 {
        ON_CHAIN_SCORE_NEUTRAL
    } else {
        let activity = saturating_log_score(inputs.trade_count, ON_CHAIN_TRADE_SATURATION) / 100.0;
        (ON_CHAIN_SCORE_NEUTRAL + 35.0 * activity).clamp(ON_CHAIN_SCORE_NEUTRAL, 85.0)
    };

    let sentiment_score = if review_count == 0 {
        ON_CHAIN_SCORE_NEUTRAL
    } else {
        let sentiment_bias =
            (inputs.positive_reviews as f64 - inputs.negative_reviews as f64) / review_count as f64;
        let rating_bias = if inputs.avg_rating > 0.0 {
            ((inputs.avg_rating - 3.0) / 2.0).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let confidence = 1.0 - (1.0 / (1.0 + review_count as f64 * 0.4));
        let base = (ON_CHAIN_SCORE_NEUTRAL + 45.0 * (0.7 * sentiment_bias + 0.3 * rating_bias))
            .clamp(0.0, 100.0);
        (ON_CHAIN_SCORE_NEUTRAL * (1.0 - confidence) + base * confidence).clamp(0.0, 100.0)
    };

    match (inputs.trade_count > 0, review_count > 0) {
        (true, true) => (0.30 * trade_score + 0.70 * sentiment_score).clamp(0.0, 100.0),
        (true, false) => trade_score,
        (false, true) => sentiment_score,
        (false, false) => ON_CHAIN_SCORE_NEUTRAL,
    }
}

fn env_or_setting(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            load_setting_env_value(key)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
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

fn compute_relevance(
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

fn compute_label_quality(
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

fn compute_metadata_completeness(result: &SearchResult, metadata: Option<&DatasetMetadata>) -> f64 {
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

fn knapsack_select_from_batch(
    task: &DatasetSelectionTask,
    batch: &[ValuationCandidateState],
    global_offset: usize,
    batch_index: usize,
) -> Option<DatasetCollectionSelectionPlan> {
    if batch.is_empty() {
        return None;
    }

    let budget_cap = normalized_budget_cap(task, batch);
    let max_total_size_bytes = normalized_max_total_size_bytes(task, batch);
    if max_total_size_bytes < task.min_total_size_bytes {
        return None;
    }

    let price_step = derive_price_step(budget_cap);
    let budget_units = discretize_price_units(budget_cap, price_step)?;
    let batch_candidates: Vec<SelectionBatchCandidate> = batch
        .iter()
        .enumerate()
        .filter_map(|(local_index, candidate)| {
            let size_bytes =
                candidate_size_bytes(&candidate.valuation.result, candidate.metadata.as_ref());
            let price_amount =
                candidate_price_amount(&candidate.valuation.result, candidate.metadata.as_ref());
            let total_value = candidate_total_value(candidate);

            if size_bytes == 0
                || size_bytes > max_total_size_bytes
                || price_amount > budget_cap + SELECTION_VALUE_EPSILON
                || total_value <= 0.0
            {
                return None;
            }

            Some(SelectionBatchCandidate {
                global_index: global_offset + local_index,
                price_amount,
                price_units: discretize_price_units(price_amount, price_step)?,
                size_bytes,
                total_value,
            })
        })
        .collect();

    if batch_candidates.is_empty() {
        return None;
    }

    let mut states = vec![HashMap::<u64, CollectionDpState>::new(); budget_units + 1];
    states[0].insert(0, CollectionDpState::default());

    for candidate in &batch_candidates {
        for spent_units in (0..=budget_units.saturating_sub(candidate.price_units)).rev() {
            if states[spent_units].is_empty() {
                continue;
            }

            let existing_states: Vec<CollectionDpState> =
                states[spent_units].values().cloned().collect();
            for state in existing_states {
                let new_size = state.total_size_bytes.saturating_add(candidate.size_bytes);
                if new_size > max_total_size_bytes {
                    continue;
                }

                let new_spent_units = spent_units + candidate.price_units;
                let new_state = CollectionDpState {
                    total_price: state.total_price + candidate.price_amount,
                    total_size_bytes: new_size,
                    total_value: state.total_value + candidate.total_value,
                    chosen_indices: {
                        let mut chosen_indices = state.chosen_indices.clone();
                        chosen_indices.push(candidate.global_index);
                        chosen_indices
                    },
                };

                let replace = states[new_spent_units]
                    .get(&new_size)
                    .map(|current| is_better_dp_state(&new_state, current))
                    .unwrap_or(true);
                if replace {
                    states[new_spent_units].insert(new_size, new_state);
                }
            }
        }

        for frontier in &mut states {
            prune_selection_frontier(frontier);
        }
    }

    let mut best: Option<(usize, CollectionDpState)> = None;
    for (spent_units, frontier) in states.into_iter().enumerate() {
        for state in frontier.into_values() {
            if state.chosen_indices.is_empty()
                || state.total_size_bytes < task.min_total_size_bytes
                || state.total_size_bytes > max_total_size_bytes
            {
                continue;
            }

            let replace = best
                .as_ref()
                .map(|(best_spent_units, best_state)| {
                    is_better_collection_solution(
                        &state,
                        spent_units,
                        best_state,
                        *best_spent_units,
                    )
                })
                .unwrap_or(true);
            if replace {
                best = Some((spent_units, state));
            }
        }
    }

    best.map(|(_, state)| DatasetCollectionSelectionPlan {
        candidate_indices: state.chosen_indices,
        total_price: state.total_price,
        total_size_bytes: state.total_size_bytes,
        total_value: state.total_value,
        batch_index,
        batch_start_rank: global_offset + 1,
        batch_end_rank: global_offset + batch.len(),
        price_step,
    })
}

fn build_selected_collection(
    plan: &DatasetCollectionSelectionPlan,
    candidates: &[ValuationCandidateState],
) -> DatasetCollectionSelection {
    let mut ranked_candidates: Vec<(f64, DatasetValuation)> = plan
        .candidate_indices
        .iter()
        .filter_map(|index| {
            candidates.get(*index).map(|candidate| {
                (
                    candidate_total_value(candidate),
                    candidate.valuation.clone(),
                )
            })
        })
        .collect();
    ranked_candidates.sort_by(|(left_value, _), (right_value, _)| {
        right_value
            .partial_cmp(left_value)
            .unwrap_or(Ordering::Equal)
    });
    let datasets = ranked_candidates
        .into_iter()
        .map(|(_, valuation)| valuation)
        .collect();

    DatasetCollectionSelection {
        datasets,
        total_price: plan.total_price,
        total_size_bytes: plan.total_size_bytes,
        total_value: plan.total_value,
        batch_index: plan.batch_index,
        batch_start_rank: plan.batch_start_rank,
        batch_end_rank: plan.batch_end_rank,
        price_step: plan.price_step,
    }
}

fn select_representative_dataset(
    collection: &DatasetCollectionSelection,
) -> Option<DatasetValuation> {
    collection.datasets.first().cloned()
}

fn is_better_dp_state(candidate: &CollectionDpState, current: &CollectionDpState) -> bool {
    candidate.total_value > current.total_value + SELECTION_VALUE_EPSILON
        || ((candidate.total_value - current.total_value).abs() <= SELECTION_VALUE_EPSILON
            && candidate.total_price + SELECTION_VALUE_EPSILON < current.total_price)
}

fn is_better_collection_solution(
    candidate: &CollectionDpState,
    candidate_spent_units: usize,
    current: &CollectionDpState,
    current_spent_units: usize,
) -> bool {
    if candidate.total_value > current.total_value + SELECTION_VALUE_EPSILON {
        return true;
    }
    if (candidate.total_value - current.total_value).abs() > SELECTION_VALUE_EPSILON {
        return false;
    }
    if candidate_spent_units != current_spent_units {
        return candidate_spent_units < current_spent_units;
    }
    if (candidate.total_price - current.total_price).abs() > SELECTION_VALUE_EPSILON {
        return candidate.total_price < current.total_price;
    }
    candidate.total_size_bytes > current.total_size_bytes
}

fn prune_selection_frontier(frontier: &mut HashMap<u64, CollectionDpState>) {
    if frontier.len() <= 1 {
        return;
    }

    let mut entries: Vec<(u64, CollectionDpState)> = frontier.drain().collect();
    entries.sort_by(|(left_size, left_state), (right_size, right_state)| {
        left_size
            .cmp(right_size)
            .then_with(|| {
                right_state
                    .total_value
                    .partial_cmp(&left_state.total_value)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| {
                left_state
                    .total_price
                    .partial_cmp(&right_state.total_price)
                    .unwrap_or(Ordering::Equal)
            })
    });

    let mut best_value_so_far = f64::NEG_INFINITY;
    for (size, state) in entries {
        if state.total_value <= best_value_so_far + SELECTION_VALUE_EPSILON {
            continue;
        }
        best_value_so_far = best_value_so_far.max(state.total_value);
        frontier.insert(size, state);
    }
}

fn normalized_budget_cap(task: &DatasetSelectionTask, batch: &[ValuationCandidateState]) -> f64 {
    task.max_total_price
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_else(|| {
            batch
                .iter()
                .map(|candidate| {
                    candidate_price_amount(&candidate.valuation.result, candidate.metadata.as_ref())
                })
                .sum::<f64>()
        })
}

fn normalized_max_total_size_bytes(
    task: &DatasetSelectionTask,
    batch: &[ValuationCandidateState],
) -> u64 {
    task.max_total_size_bytes.unwrap_or_else(|| {
        batch.iter().fold(0_u64, |acc, candidate| {
            acc.saturating_add(candidate_size_bytes(
                &candidate.valuation.result,
                candidate.metadata.as_ref(),
            ))
        })
    })
}

fn derive_price_step(budget_cap: f64) -> f64 {
    const NICE_STEPS: &[f64] = &[0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0, 50.0, 100.0, 500.0];

    if !budget_cap.is_finite() || budget_cap <= 0.0 {
        return 0.01;
    }

    let raw_step = (budget_cap / MAX_SELECTION_BUDGET_BUCKETS as f64).max(0.01);
    NICE_STEPS
        .iter()
        .copied()
        .find(|step| *step >= raw_step)
        .unwrap_or_else(|| raw_step.ceil().max(1.0))
}

fn discretize_price_units(amount: f64, step: f64) -> Option<usize> {
    if !amount.is_finite() || !step.is_finite() || step <= 0.0 {
        return None;
    }
    Some((amount / step).ceil().max(0.0) as usize)
}

fn candidate_price_amount(result: &SearchResult, metadata: Option<&DatasetMetadata>) -> f64 {
    metadata
        .map(|metadata| metadata.price.amount)
        .unwrap_or(result.price.amount)
        .max(0.0)
}

fn candidate_size_bytes(result: &SearchResult, metadata: Option<&DatasetMetadata>) -> u64 {
    candidate_shape(result, metadata).1
}

fn candidate_sample_count(result: &SearchResult, metadata: Option<&DatasetMetadata>) -> u64 {
    candidate_shape(result, metadata).0
}

fn candidate_average_value_score(valuation: &DatasetValuation) -> f64 {
    valuation.final_score.max(0.0)
}

fn candidate_contribution_value(
    result: &SearchResult,
    metadata: Option<&DatasetMetadata>,
    valuation: &DatasetValuation,
) -> f64 {
    candidate_sample_count(result, metadata) as f64 * candidate_average_value_score(valuation)
}

fn candidate_total_value(candidate: &ValuationCandidateState) -> f64 {
    candidate_contribution_value(
        &candidate.valuation.result,
        candidate.metadata.as_ref(),
        &candidate.valuation,
    )
}

fn describe_size_interval(min_size_bytes: u64, max_size_bytes: Option<u64>) -> String {
    match max_size_bytes {
        Some(max_size_bytes) => format!("[{min_size_bytes}, {max_size_bytes}] bytes"),
        None => format!("[{min_size_bytes}, unbounded] bytes"),
    }
}

fn parse_budget_amount_str(value: &str) -> Option<f64> {
    let mut number = String::new();
    let mut started = false;
    let mut seen_dot = false;

    for ch in value.chars() {
        if ch.is_ascii_digit() {
            number.push(ch);
            started = true;
            continue;
        }
        if ch == '.' && started && !seen_dot {
            number.push(ch);
            seen_dot = true;
            continue;
        }
        if ch == ',' && started {
            continue;
        }
        if started {
            break;
        }
    }

    if number.is_empty() {
        None
    } else {
        number.parse::<f64>().ok()
    }
}

fn supports_sampling_without_shape_estimate(result: &SearchResult) -> bool {
    matches!(result.source, DataSource::GuixuHub)
}

fn build_valuation_explanation(valuation: &DatasetValuation) -> String {
    let academic = is_academic_result(&valuation.result);
    let mut parts = vec![format!("coarse={:.1}", valuation.coarse_score)];

    parts.push(format!("relevance={:.1}", valuation.relevance));
    if academic {
        parts.push(format!("citation={:.1}", valuation.schema_fit));
        parts.push(format!("venue={:.1}", valuation.scale_score));
        parts.push(format!("recency={:.1}", valuation.label_quality));
        parts.push(format!("abstract={:.1}", valuation.metadata_completeness));
    } else {
        parts.push(format!("schema={:.1}", valuation.schema_fit));
        parts.push(format!("scale={:.1}", valuation.scale_score));
        parts.push(format!("label_quality={:.1}", valuation.label_quality));
        parts.push(format!(
            "metadata_completeness={:.1}",
            valuation.metadata_completeness
        ));
    }
    parts.push(format!("onchain={:.1}", valuation.on_chain_score));

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
    if let Some(reason) = &valuation.sample_failure_reason {
        parts.push(format!("sample_reason={reason}"));
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

fn normalized_metadata_text(metadata: &DatasetMetadata) -> String {
    let mut parts = vec![metadata.title.to_lowercase()];
    if let Some(description) = metadata.description.as_deref() {
        parts.push(description.to_lowercase());
    }
    if !metadata.tags.is_empty() {
        parts.push(metadata.tags.join(" ").to_lowercase());
    }
    if !metadata.schema.columns.is_empty() {
        parts.push(
            metadata
                .schema
                .columns
                .iter()
                .map(|column| column.name.to_lowercase())
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    parts.push(format!("{:?}", metadata.data_type).to_lowercase());
    parts.join(" ")
}

fn normalized_dataset_text(result: &SearchResult, metadata: Option<&DatasetMetadata>) -> String {
    build_dataset_text(result, metadata).to_lowercase()
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
        seller_endpoint: None,
        source_attributes: m.source_attributes.clone(),
        governance: None,
        provider_meta: None,
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

fn normalized_search_result_text(result: &SearchResult) -> String {
    searchable_result_text(result)
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

#[cfg(test)]
mod engine_unit_tests {
    use super::{
        compose_coarse_score, score_on_chain_signal, OnChainScoreInputs,
        ON_CHAIN_COARSE_ADJUST_WEIGHT, ON_CHAIN_SCORE_NEUTRAL,
    };

    #[test]
    fn coarse_score_keeps_neutral_on_chain_signal_aligned() {
        let baseline = 0.50 * 82.0 + 0.15 * 74.0 + 0.15 * 68.0 + 0.10 * 57.0 + 0.05 * 91.0;
        let coarse =
            compose_coarse_score(82.0, 74.0, 68.0, 57.0, 91.0, ON_CHAIN_SCORE_NEUTRAL, 0.0);
        assert!((coarse - baseline).abs() < 1e-6);
    }

    #[test]
    fn coarse_score_applies_centered_on_chain_adjustment() {
        let neutral =
            compose_coarse_score(80.0, 70.0, 60.0, 50.0, 40.0, ON_CHAIN_SCORE_NEUTRAL, 0.0);
        let boosted = compose_coarse_score(80.0, 70.0, 60.0, 50.0, 40.0, 90.0, 0.0);
        let penalized = compose_coarse_score(80.0, 70.0, 60.0, 50.0, 40.0, 10.0, 0.0);

        assert!((boosted - neutral - ON_CHAIN_COARSE_ADJUST_WEIGHT * 40.0).abs() < 1e-6);
        assert!((neutral - penalized - ON_CHAIN_COARSE_ADJUST_WEIGHT * 40.0).abs() < 1e-6);
    }

    #[test]
    fn on_chain_signal_is_neutral_without_trade_or_reviews() {
        assert_eq!(
            score_on_chain_signal(&OnChainScoreInputs::default()),
            ON_CHAIN_SCORE_NEUTRAL
        );
    }

    #[test]
    fn on_chain_signal_rewards_positive_trade_activity() {
        let score = score_on_chain_signal(&OnChainScoreInputs {
            trade_count: 36,
            avg_rating: 4.8,
            positive_reviews: 8,
            neutral_reviews: 1,
            negative_reviews: 0,
        });

        assert!(
            score > 70.0,
            "expected strong positive on-chain score, got {score}"
        );
    }

    #[test]
    fn on_chain_signal_penalizes_negative_reviews() {
        let score = score_on_chain_signal(&OnChainScoreInputs {
            trade_count: 28,
            avg_rating: 1.4,
            positive_reviews: 1,
            neutral_reviews: 0,
            negative_reviews: 9,
        });

        assert!(score < 40.0, "expected low on-chain score, got {score}");
    }
}

#[cfg(test)]
mod selection_tests {
    use super::{
        knapsack_select_from_batch, metadata_to_search_result, DatasetSelectionTask,
        DatasetValuation, DatasetValuationConfig, ProxyUtilityApplyMode, ProxyUtilityReport,
        RankedResult, SampleEvaluationOutcome, SampleEvaluator, SearchEngine, SearchOutput,
        ValuationCandidateState,
    };
    use crate::intent::IntentParser;
    use crate::vector_index::VectorIndex;
    use anyhow::Result;
    use chrono::Utc;
    use data_core::feedback::CommunitySignal;
    use data_core::metadata::{DatasetMetadata, Provenance};
    use data_core::types::{
        AccessMode, ColumnDef, DataType, DatasetCid, DatasetSchema, Did, License, Price,
        SearchResult,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn test_task(
        max_total_price: f64,
        min_total_size_bytes: u64,
        max_total_size_bytes: u64,
    ) -> DatasetSelectionTask {
        DatasetSelectionTask {
            task_description: "cat detector".into(),
            task_type: "classification".into(),
            required_columns: vec!["label".into()],
            target_entity: Some("cat".into()),
            required_data_type: Some(DataType::Tabular),
            max_total_price: Some(max_total_price),
            min_total_size_bytes,
            max_total_size_bytes: Some(max_total_size_bytes),
        }
    }

    fn test_metadata(
        cid_suffix: &str,
        title: &str,
        description: &str,
        row_count: u64,
        size_bytes: u64,
        price_amount: f64,
    ) -> DatasetMetadata {
        DatasetMetadata {
            cid: DatasetCid(format!("cid-{cid_suffix}")),
            info_hash: Some(format!("hash-{cid_suffix}")),
            title: title.into(),
            description: Some(description.into()),
            tags: vec!["cat".into()],
            data_type: DataType::Tabular,
            schema: DatasetSchema {
                columns: vec![
                    ColumnDef {
                        name: "sample_id".into(),
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
                row_count: row_count.max(1),
                size_bytes,
            },
            stats: None,
            video_meta: None,
            access: AccessMode::Open,
            price: Price::usdc(price_amount),
            license: License {
                spdx_id: "CC-BY-4.0".into(),
                commercial_use: true,
                derivative_allowed: true,
            },
            provider: Did("did:key:test".into()),
            signature: "sig".into(),
            provenance: Provenance::Original,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            verifiable_credential: None,
            source_attributes: None,
            version: None,
            previous_version: None,
        }
    }

    fn test_candidate_state(
        cid_suffix: &str,
        title: &str,
        description: &str,
        row_count: u64,
        size_bytes: u64,
        price_amount: f64,
        final_score: f64,
    ) -> ValuationCandidateState {
        let metadata = test_metadata(
            cid_suffix,
            title,
            description,
            row_count,
            size_bytes,
            price_amount,
        );
        let result = metadata_to_search_result(&metadata);
        ValuationCandidateState {
            metadata: Some(metadata),
            valuation: DatasetValuation {
                result,
                coarse_score: final_score,
                final_score,
                relevance: final_score,
                schema_fit: 100.0,
                scale_score: 100.0,
                label_quality: 100.0,
                metadata_completeness: 100.0,
                on_chain_score: 50.0,
                metadata_resolved: true,
                sample_plan: None,
                proxy_utility: None,
                sample_failure_reason: None,
                explanation: String::new(),
            },
        }
    }

    fn test_ranked_result(metadata: &DatasetMetadata) -> RankedResult {
        RankedResult {
            result: metadata_to_search_result(metadata),
            rank_score: 0.0,
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

    async fn test_engine() -> SearchEngine {
        SearchEngine::new(
            VectorIndex::init().await.expect("VectorIndex init failed"),
            IntentParser,
            vec![],
        )
    }

    struct StubSampleEvaluator {
        scores: HashMap<String, f64>,
        seen: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl SampleEvaluator for StubSampleEvaluator {
        async fn evaluate_sample(
            &self,
            result: &SearchResult,
            _metadata: &DatasetMetadata,
            _task: &DatasetSelectionTask,
            plan: &super::SamplePlan,
        ) -> Result<SampleEvaluationOutcome> {
            self.seen.lock().unwrap().push(result.cid.0.clone());
            let utility_score = self.scores.get(&result.cid.0).copied().unwrap_or(50.0);
            Ok(SampleEvaluationOutcome::scored(ProxyUtilityReport {
                utility_score,
                apply_mode: ProxyUtilityApplyMode::OverrideFinal,
                proxy_metric_name: "stub".into(),
                proxy_metric_value: utility_score / 100.0,
                sampled_rows: plan.estimated_rows.max(1),
                sampled_bytes: plan.estimated_bytes.max(1),
                notes: None,
            }))
        }
    }

    #[test]
    fn batch_collection_selection_maximizes_total_value() {
        let batch = vec![
            test_candidate_state("a", "Cat A", "cat candidate a", 5, 30, 1.0, 90.0),
            test_candidate_state("b", "Cat B", "cat candidate b", 100, 10, 2.0, 60.0),
            test_candidate_state("c", "Cat C", "cat candidate c", 50, 20, 1.0, 50.0),
        ];
        let task = test_task(3.0, 20, 40);

        let selected = knapsack_select_from_batch(&task, &batch, 0, 0).unwrap();

        assert_eq!(selected.candidate_indices, vec![1, 2]);
        assert!((selected.total_price - 3.0).abs() < 1e-6);
        assert_eq!(selected.total_size_bytes, 30);
    }

    #[tokio::test]
    async fn value_search_output_advances_to_next_batch_when_first_batch_infeasible() {
        let metadata = vec![
            test_metadata("a", "Cat shortlist A", "cat dataset a", 10, 10, 1.0),
            test_metadata("b", "Cat shortlist B", "cat dataset b", 10, 10, 1.0),
            test_metadata("c", "Archive C", "generic archive c", 20, 20, 2.0),
            test_metadata("d", "Archive D", "generic archive d", 15, 15, 2.0),
        ];
        let search_output = SearchOutput {
            results: metadata.iter().map(test_ranked_result).collect(),
            errors: vec![],
            profile: None,
        };
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sample_evaluator = StubSampleEvaluator {
            scores: HashMap::from([
                ("cid-a".into(), 95.0),
                ("cid-b".into(), 90.0),
                ("cid-c".into(), 80.0),
                ("cid-d".into(), 70.0),
            ]),
            seen: seen.clone(),
        };
        let task = test_task(5.0, 30, 45);
        let config = DatasetValuationConfig {
            coarse_top_k: 2,
            ..DatasetValuationConfig::default()
        };

        let output = test_engine()
            .value_search_output(
                &search_output,
                &metadata,
                None,
                Some(&sample_evaluator),
                &task,
                &config,
            )
            .await
            .unwrap();

        let selected_collection = output.selected_collection.unwrap();
        let selected_cids: Vec<String> = selected_collection
            .datasets
            .iter()
            .map(|dataset| dataset.result.cid.0.clone())
            .collect();

        assert_eq!(selected_collection.batch_index, 1);
        assert_eq!(selected_collection.batch_start_rank, 1);
        assert_eq!(selected_collection.batch_end_rank, 4);
        assert_eq!(
            selected_cids,
            vec![
                "cid-c".to_string(),
                "cid-d".to_string(),
                "cid-a".to_string(),
            ]
        );
        assert_eq!(seen.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn value_search_output_stops_sampling_after_first_feasible_batch() {
        let metadata = vec![
            test_metadata("a", "Cat shortlist A", "cat dataset a", 20, 20, 1.0),
            test_metadata("b", "Cat shortlist B", "cat dataset b", 20, 20, 1.0),
            test_metadata("c", "Archive C", "generic archive c", 30, 30, 2.0),
        ];
        let search_output = SearchOutput {
            results: metadata.iter().map(test_ranked_result).collect(),
            errors: vec![],
            profile: None,
        };
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sample_evaluator = StubSampleEvaluator {
            scores: HashMap::from([
                ("cid-a".into(), 90.0),
                ("cid-b".into(), 88.0),
                ("cid-c".into(), 70.0),
            ]),
            seen: seen.clone(),
        };
        let task = test_task(3.0, 35, 45);
        let config = DatasetValuationConfig {
            coarse_top_k: 2,
            ..DatasetValuationConfig::default()
        };

        let output = test_engine()
            .value_search_output(
                &search_output,
                &metadata,
                None,
                Some(&sample_evaluator),
                &task,
                &config,
            )
            .await
            .unwrap();

        let selected_collection = output.selected_collection.unwrap();
        assert_eq!(selected_collection.batch_index, 0);
        assert_eq!(seen.lock().unwrap().len(), 2);

        let skipped_candidate = output
            .candidates
            .iter()
            .find(|candidate| candidate.result.cid.0 == "cid-c")
            .unwrap();
        assert_eq!(
            skipped_candidate.sample_failure_reason.as_deref(),
            Some("sample evaluation skipped: collection selection succeeded in an earlier batch")
        );
    }
}
