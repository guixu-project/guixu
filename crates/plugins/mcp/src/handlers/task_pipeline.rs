// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use anyhow::Result;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use data_core::feedback::CommunitySignal;
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::{AccessMode, DatasetCid, SearchResult, SkillCapability, SourceFamily};
use data_search::engine::{
    DatasetSelectionTask, DatasetValuation, DatasetValuationConfig, MetadataResolver,
    ProxyUtilityApplyMode, SampleEvaluationOutcome, SampleEvaluator, SearchFilters, SearchOutput,
    ON_CHAIN_COARSE_ADJUST_WEIGHT, ON_CHAIN_SCORE_NEUTRAL,
};
use data_search::adapters::{load_open_data_skills, SkillProvider};
use data_search::intent::QueryProfile;
use data_search::sample_eval::{
    ChainedSampleDownloader, DownloadedSample, GuixuHubSampleDownloader, LlmSampleJudge,
    LocalHeuristicProxyScorer, ProxyLabelPropagationConfig, ProxyLabelPropagationEvaluator,
    ProxyScreeningReport, SampleDownloader, SampleJudgeReport, SampleRequirements,
    SampleRequirementsPlanner, SeedRecordJudge, SeedRecordJudgeReport, StagedSampleEvaluator,
    StagedSampleEvaluatorConfig,
};
use data_search::skill_sample_downloader::{SkillSampleConfig, SkillSampleDownloader};

use crate::server::AppState;
use crate::state::ToolProfile;

const SAMPLED_SCORE_BASELINE_BONUS: f64 = 72.0;
const SAMPLED_SCORE_SCALE: f64 = 0.28;
const UNSAMPLED_SCORE_BASELINE_WITH_SAMPLED_COHORT: f64 = 45.0;
const UNSAMPLED_SCORE_SCALE_WITH_SAMPLED_COHORT: f64 = 0.24;
const UNSAMPLED_SCORE_BASELINE_NO_SAMPLED_COHORT: f64 = 60.0;
const UNSAMPLED_SCORE_SCALE_NO_SAMPLED_COHORT: f64 = 0.35;

struct SearchResultMetadataResolver;

#[async_trait]
impl MetadataResolver for SearchResultMetadataResolver {
    async fn resolve_metadata(&self, result: &SearchResult) -> Result<Option<DatasetMetadata>> {
        Ok(Some(DatasetMetadata {
            cid: result.cid.clone(),
            info_hash: None,
            title: result.title.clone(),
            description: result.description.clone(),
            tags: result.tags.clone(),
            data_type: result.data_type,
            schema: result.schema.clone(),
            stats: None,
            video_meta: None,
            access: if result.price.is_free() {
                AccessMode::Open
            } else {
                AccessMode::Paid
            },
            price: result.price.clone(),
            license: result.license.clone(),
            provider: result.provider.clone(),
            signature: String::new(),
            provenance: Provenance::Original,
            created_at: result.created_at,
            updated_at: result.created_at,
            verifiable_credential: None,
            source_attributes: result.source_attributes.clone(),
        }))
    }
}

#[derive(Default)]
struct HeuristicSampleRequirementsPlanner;

#[async_trait]
impl SampleRequirementsPlanner for HeuristicSampleRequirementsPlanner {
    async fn plan_requirements(&self, task: &DatasetSelectionTask) -> Result<SampleRequirements> {
        Ok(build_heuristic_sample_requirements(task))
    }
}

struct ScreeningSimilarityJudge;

#[async_trait]
impl LlmSampleJudge for ScreeningSimilarityJudge {
    async fn judge_sample(
        &self,
        _result: &SearchResult,
        _metadata: &DatasetMetadata,
        _task: &DatasetSelectionTask,
        _requirements: &SampleRequirements,
        _sample: &data_search::sample_eval::DownloadedSample,
        screening: &ProxyScreeningReport,
    ) -> Result<SampleJudgeReport> {
        Ok(SampleJudgeReport {
            utility_score: screening.similarity_score.clamp(0.0, 100.0),
            rationale: format!(
                "local heuristic sample utility derived from proxy screening: {}",
                screening.rationale
            ),
        })
    }
}

enum RuntimeSampleEvaluator {
    RandomSeed(ProxyLabelPropagationEvaluator),
    Hybrid {
        image_seed: ProxyLabelPropagationEvaluator,
        staged: StagedSampleEvaluator,
    },
}

#[async_trait]
impl SampleEvaluator for RuntimeSampleEvaluator {
    async fn evaluate_sample(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        task: &DatasetSelectionTask,
        plan: &data_search::engine::SamplePlan,
    ) -> Result<SampleEvaluationOutcome> {
        match self {
            Self::RandomSeed(evaluator) => {
                evaluator
                    .evaluate_sample(result, metadata, task, plan)
                    .await
            }
            Self::Hybrid { image_seed, staged } => {
                if matches!(metadata.data_type, data_core::types::DataType::Image)
                    || matches!(result.data_type, data_core::types::DataType::Image)
                {
                    image_seed
                        .evaluate_sample(result, metadata, task, plan)
                        .await
                } else {
                    staged.evaluate_sample(result, metadata, task, plan).await
                }
            }
        }
    }
}

struct RuntimeSeedJudge;

#[async_trait]
impl SeedRecordJudge for RuntimeSeedJudge {
    async fn judge_records(
        &self,
        _result: &SearchResult,
        _metadata: &DatasetMetadata,
        _task: &DatasetSelectionTask,
        _sample: &data_search::sample_eval::DownloadedSample,
        _records: &[data_search::sample_eval::SampleRecord],
    ) -> Result<SeedRecordJudgeReport> {
        anyhow::bail!("seed record judging unavailable — using heuristic evaluation only")
    }
}

fn sample_contains_image_records(sample: &DownloadedSample) -> bool {
    sample.records.iter().any(|record| {
        record
            .metadata
            .get("local_image_path")
            .and_then(|value| value.as_str())
            .is_some()
            || record
                .metadata
                .get("kind")
                .and_then(|value| value.as_str())
                .is_some_and(|kind| kind.eq_ignore_ascii_case("image"))
            || record
                .metadata
                .get("local_image_mime_type")
                .and_then(|value| value.as_str())
                .is_some_and(|mime| mime.starts_with("image/"))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StopAfter {
    IntentParse,
    DatasetSearch,
    DatasetEvaluate,
}

impl StopAfter {
    fn as_str(self) -> &'static str {
        match self {
            Self::IntentParse => "intent_parse",
            Self::DatasetSearch => "dataset_search",
            Self::DatasetEvaluate => "dataset_evaluate",
        }
    }
}

fn parse_stop_after(args: &Value) -> Result<StopAfter> {
    if let Some(value) = args.get("stop_after").and_then(|v| v.as_str()) {
        return match value.trim() {
            "intent_parse" => Ok(StopAfter::IntentParse),
            "dataset_search" => Ok(StopAfter::DatasetSearch),
            "dataset_evaluate" => Ok(StopAfter::DatasetEvaluate),
            other => anyhow::bail!(
                "invalid stop_after '{other}': expected intent_parse, dataset_search, or dataset_evaluate"
            ),
        };
    }

    let pipeline = args.get("pipeline").cloned().unwrap_or_default();
    let run_evaluate = pipeline
        .get("dataset_evaluate")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let run_search = pipeline
        .get("dataset_search")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    if run_evaluate {
        Ok(StopAfter::DatasetEvaluate)
    } else if run_search {
        Ok(StopAfter::DatasetSearch)
    } else {
        Ok(StopAfter::IntentParse)
    }
}

fn compact_intent(profile: &QueryProfile) -> Value {
    json!({
        "task_type": profile.task_type,
        "task_description": profile.task_description,
        "target_entity": profile.target_entity,
        "keywords": profile.keywords,
        "data_standard": {
            "sample_unit": profile.data_standard.sample_unit,
            "budget": profile.data_standard.budget,
            "max_latency_secs": profile.data_standard.max_latency_secs,
            "min_dataset_size_bytes": profile.data_standard.min_dataset_size_bytes,
            "max_dataset_size_bytes": profile.data_standard.max_dataset_size_bytes,
            "canonical_columns": profile.data_standard.canonical_columns,
            "extra_columns": profile.data_standard.extra_columns,
        }
    })
}

fn compact_candidate(result: &Value) -> Value {
    json!({
        "cid": result.get("cid").cloned().unwrap_or(Value::Null),
        "title": result.get("title").cloned().unwrap_or(Value::Null),
        "description": result.get("description").cloned().unwrap_or(Value::Null),
        "source": result.get("source").cloned().unwrap_or(Value::Null),
        "data_type": result.get("data_type").cloned().unwrap_or(Value::Null),
        "price": result.get("price").cloned().unwrap_or(Value::Null),
        "schema": result.get("schema").cloned().unwrap_or_else(|| json!({})),
        "rank": result.get("rank").cloned().unwrap_or(Value::Null),
        "rank_score": result.get("rank_score").cloned().unwrap_or(Value::Null),
    })
}

fn compact_selected_dataset(result: &Value) -> Value {
    json!({
        "cid": result.get("cid").cloned().unwrap_or(Value::Null),
        "title": result.get("title").cloned().unwrap_or(Value::Null),
        "source": result.get("source").cloned().unwrap_or(Value::Null),
        "data_type": result.get("data_type").cloned().unwrap_or(Value::Null),
        "price": result.get("price").cloned().unwrap_or(Value::Null),
        "schema": result.get("schema").cloned().unwrap_or_else(|| json!({})),
        "evaluation_mode": result.get("evaluation_mode").cloned().unwrap_or(Value::Null),
        "final_score": result.get("final_score").cloned().unwrap_or(Value::Null),
        "raw_final_score": result.get("raw_final_score").cloned().unwrap_or(Value::Null),
        "coarse_score": result.get("coarse_score").cloned().unwrap_or(Value::Null),
        "on_chain_score": result.get("on_chain_score").cloned().unwrap_or(Value::Null),
        "final_score_breakdown": result
            .get("final_score_breakdown")
            .cloned()
            .unwrap_or(Value::Null),
        "sample_scored": result.get("sample_scored").cloned().unwrap_or(Value::Null),
        "sample_failure_reason": result
            .get("sample_failure_reason")
            .cloned()
            .unwrap_or(Value::Null),
        "tcv_score": result.get("tcv_score").cloned().unwrap_or(Value::Null),
        "verdict": result.get("verdict").cloned().unwrap_or(Value::Null),
    })
}

fn parse_json_or_text(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| json!({ "text": raw }))
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

fn parse_budget_amount(value: Option<&Value>) -> Option<f64> {
    let raw = value?;
    raw.as_f64()
        .or_else(|| raw.as_str().and_then(parse_budget_amount_str))
}

fn parse_string_array(obj: &Value, plural_key: &str, singular_key: &str) -> Vec<String> {
    obj.get(plural_key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items.iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect()
        })
        .or_else(|| {
            obj.get(singular_key)
                .and_then(|value| value.as_str())
                .map(|value| vec![value.to_string()])
        })
        .unwrap_or_default()
}

fn parse_enum_array<T>(obj: &Value, plural_key: &str, singular_key: &str) -> Vec<T>
where
    T: DeserializeOwned,
{
    obj.get(plural_key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items.iter()
                .filter_map(|value| serde_json::from_value(value.clone()).ok())
                .collect()
        })
        .or_else(|| {
            obj.get(singular_key)
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok())
                .map(|value| vec![value])
        })
        .unwrap_or_default()
}

fn collect_search_filters(search_args: &Value) -> (SearchFilters, usize) {
    let filter_obj = search_args.get("filters").cloned().unwrap_or_default();
    let filters = SearchFilters {
        topic: filter_obj
            .get("topic")
            .and_then(|v| v.as_str())
            .map(String::from),
        min_rows: filter_obj.get("min_rows").and_then(|v| v.as_u64()),
        max_price: filter_obj.get("max_price").and_then(|v| v.as_f64()),
        license: filter_obj
            .get("license")
            .and_then(|v| v.as_str())
            .map(String::from),
        min_quality: filter_obj.get("min_quality").and_then(|v| v.as_f64()),
        skill_ids: parse_string_array(&filter_obj, "skill_ids", "skill_id"),
        source_families: parse_enum_array::<SourceFamily>(
            &filter_obj,
            "source_families",
            "source_family",
        ),
        required_capabilities: parse_enum_array::<SkillCapability>(
            &filter_obj,
            "required_capabilities",
            "required_capability",
        ),
        chain: filter_obj
            .get("chain")
            .and_then(|v| v.as_str())
            .map(String::from),
        protocol: filter_obj
            .get("protocol")
            .and_then(|v| v.as_str())
            .map(String::from),
        asset: filter_obj
            .get("asset")
            .and_then(|v| v.as_str())
            .map(String::from),
        category: filter_obj
            .get("category")
            .and_then(|v| v.as_str())
            .map(String::from),
        free_only: filter_obj.get("free_only").and_then(|v| v.as_bool()),
    };
    let limit = search_args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;
    (filters, limit)
}

fn default_required_columns(profile: &QueryProfile) -> Vec<String> {
    let mut columns = profile.data_standard.canonical_columns.clone();
    for column in &profile.data_standard.extra_columns {
        if !columns.contains(column) {
            columns.push(column.clone());
        }
    }
    columns.retain(|column| !column.trim().is_empty());
    columns
}

fn evaluate_required_columns(evaluate_args: &Value, profile: &QueryProfile) -> Vec<String> {
    let explicit: Vec<String> = evaluate_args
        .get("required_columns")
        .and_then(|v| v.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::trim))
                .filter(|value| !value.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    if explicit.is_empty() {
        default_required_columns(profile)
    } else {
        explicit
    }
}

fn extract_candidate_cids(results: &[Value]) -> Vec<String> {
    results
        .iter()
        .filter_map(|result| result.get("cid").and_then(|v| v.as_str()))
        .map(String::from)
        .collect()
}

fn extract_score(value: &Value) -> f64 {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|raw| raw.parse::<f64>().ok()))
        .unwrap_or(0.0)
}

fn extract_percentage(value: &Value) -> f64 {
    value
        .as_f64()
        .or_else(|| {
            value.as_str().and_then(|raw| {
                raw.trim_end_matches('%')
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .map(|parsed| parsed / 100.0)
            })
        })
        .unwrap_or(0.0)
}

fn build_runtime_sample_downloader() -> Box<dyn SampleDownloader> {
    let hub_downloader = GuixuHubSampleDownloader::default().with_record_limit(12);

    // Load skill-based sample configs from JSON skill files.
    let skill_configs: Vec<SkillSampleConfig> = load_open_data_skills()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|spec| {
            let sample = spec.sample?;
            let provider_base_url = match &spec.provider {
                SkillProvider::HttpSearch { base_url, .. } => Some(base_url.as_str()),
                _ => None,
            };
            Some(SkillSampleConfig::from_provider(&sample, provider_base_url))
        })
        .collect();

    if skill_configs.is_empty() {
        Box::new(hub_downloader)
    } else {
        let skill_downloader = SkillSampleDownloader::new(skill_configs);
        Box::new(ChainedSampleDownloader::new(vec![
            Box::new(skill_downloader),
            Box::new(hub_downloader),
        ]))
    }
}

fn build_runtime_staged_evaluator() -> StagedSampleEvaluator {
    StagedSampleEvaluator::with_config(
        build_runtime_sample_downloader(),
        Box::new(HeuristicSampleRequirementsPlanner),
        Box::new(LocalHeuristicProxyScorer),
        Box::new(ScreeningSimilarityJudge),
        StagedSampleEvaluatorConfig::default(),
    )
}

fn build_runtime_sample_evaluator() -> RuntimeSampleEvaluator {
    RuntimeSampleEvaluator::Hybrid {
        image_seed: ProxyLabelPropagationEvaluator::with_config(
            build_runtime_sample_downloader(),
            Box::new(RuntimeSeedJudge),
            ProxyLabelPropagationConfig {
                seed_record_count: 4,
                low_score_threshold: 35.0,
                high_score_threshold: 75.0,
                override_final_below_score: 20.0,
                selection_seed: 2026,
            },
        ),
        staged: build_runtime_staged_evaluator(),
    }
}

fn build_heuristic_sample_requirements(task: &DatasetSelectionTask) -> SampleRequirements {
    let mut required_signals = collect_requirement_terms(&task.task_description);
    if let Some(target_entity) = task.target_entity.as_deref() {
        required_signals.extend(collect_requirement_terms(target_entity));
    }
    for column in &task.required_columns {
        required_signals.extend(collect_requirement_terms(column));
    }
    match task.task_type.as_str() {
        "classification" => required_signals.push("label".into()),
        "time_series_prediction" | "forecasting" => required_signals.push("timestamp".into()),
        "video_classification" => required_signals.push("video".into()),
        "nlp" => required_signals.push("text".into()),
        _ => {}
    }

    let mut seen = HashMap::new();
    let required_signals = required_signals
        .into_iter()
        .filter(|term| !term.is_empty())
        .filter(|term| seen.insert(term.clone(), ()).is_none())
        .take(8)
        .collect::<Vec<_>>();
    let preferred_labels = if task.task_type == "classification" {
        vec!["label".into(), "positive".into(), "negative".into()]
    } else {
        vec![]
    };

    SampleRequirements {
        summary: task.task_description.clone(),
        required_signals,
        preferred_labels,
        disqualifying_signals: vec![],
    }
}

fn collect_requirement_terms(text: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "a", "an", "and", "as", "at", "be", "by", "data", "dataset", "for", "from", "if", "in",
        "into", "is", "of", "on", "or", "the", "to", "with",
    ];

    text.split(|character: char| !character.is_alphanumeric())
        .map(|term| term.trim().to_lowercase())
        .filter(|term| term.len() > 1)
        .filter(|term| !STOPWORDS.contains(&term.as_str()))
        .collect()
}

fn normalized_mix_weights(config: &DatasetValuationConfig) -> (f64, f64) {
    let metadata_weight = config.metadata_weight.max(0.0);
    let utility_weight = config.utility_weight.max(0.0);
    let total = (metadata_weight + utility_weight).max(f64::EPSILON);
    (metadata_weight / total, utility_weight / total)
}

fn final_score_from_valuation(
    valuation: &DatasetValuation,
    config: &DatasetValuationConfig,
    has_sample_scored_candidates: bool,
) -> (f64, Value) {
    let (metadata_base, metadata_mix, utility_mix, sample_bonus) =
        match valuation.proxy_utility.as_ref() {
            Some(proxy_utility) if has_sample_scored_candidates => match proxy_utility.apply_mode {
                ProxyUtilityApplyMode::Blend => {
                    let (metadata_weight, utility_weight) = normalized_mix_weights(config);
                    (
                        0.0,
                        SAMPLED_SCORE_SCALE * metadata_weight,
                        SAMPLED_SCORE_SCALE * utility_weight,
                        SAMPLED_SCORE_BASELINE_BONUS,
                    )
                }
                ProxyUtilityApplyMode::OverrideFinal => {
                    (0.0, 0.0, SAMPLED_SCORE_SCALE, SAMPLED_SCORE_BASELINE_BONUS)
                }
            },
            Some(proxy_utility) => match proxy_utility.apply_mode {
                ProxyUtilityApplyMode::Blend => {
                    let (metadata_weight, utility_weight) = normalized_mix_weights(config);
                    (0.0, metadata_weight, utility_weight, 0.0)
                }
                ProxyUtilityApplyMode::OverrideFinal => (0.0, 0.0, 1.0, 0.0),
            },
            None if has_sample_scored_candidates => (
                UNSAMPLED_SCORE_BASELINE_WITH_SAMPLED_COHORT,
                UNSAMPLED_SCORE_SCALE_WITH_SAMPLED_COHORT,
                0.0,
                0.0,
            ),
            None => (
                UNSAMPLED_SCORE_BASELINE_NO_SAMPLED_COHORT,
                UNSAMPLED_SCORE_SCALE_NO_SAMPLED_COHORT,
                0.0,
                0.0,
            ),
        };

    let metadata_components = [
        (
            "relevance",
            "Relevance",
            valuation.relevance,
            0.55,
        ),
        ("schema_fit", "Schema Fit", valuation.schema_fit, 0.15),
        ("scale_score", "Scale Score", valuation.scale_score, 0.15),
        (
            "label_quality",
            "Label Quality",
            valuation.label_quality,
            0.10,
        ),
        (
            "metadata_completeness",
            "Metadata Completeness",
            valuation.metadata_completeness,
            0.05,
        ),
    ];
    let mut components = metadata_components
        .into_iter()
        .map(|(id, label, value, weight)| {
            let contribution = metadata_mix * weight * value;
            json!({
                "id": id,
                "label": label,
                "value": value,
                "contribution": contribution,
            })
        })
        .collect::<Vec<_>>();
    let on_chain_contribution = metadata_mix
        * ON_CHAIN_COARSE_ADJUST_WEIGHT
        * (valuation.on_chain_score - ON_CHAIN_SCORE_NEUTRAL);
    components.push(json!({
        "id": "on_chain_score",
        "label": "On-Chain Score",
        "value": valuation.on_chain_score,
        "baseline": ON_CHAIN_SCORE_NEUTRAL,
        "contribution": on_chain_contribution,
    }));
    if metadata_base > 0.0 {
        components.push(json!({
            "id": "base_score",
            "label": "Ranking Baseline",
            "value": metadata_base,
            "contribution": metadata_base,
        }));
    }

    let sample_utility_score = valuation
        .proxy_utility
        .as_ref()
        .map(|proxy_utility| proxy_utility.utility_score)
        .unwrap_or(0.0);
    let sample_utility_contribution = utility_mix * sample_utility_score;
    components.push(json!({
        "id": "sample_utility",
        "label": "Sample Utility",
        "value": sample_utility_score,
        "contribution": sample_utility_contribution,
    }));
    components.push(json!({
        "id": "sample_bonus",
        "label": "Sample Verified Bonus",
        "value": sample_bonus,
        "contribution": sample_bonus,
    }));

    let weighted_sum = components
        .iter()
        .map(|component| {
            component
                .get("contribution")
                .map(extract_score)
                .unwrap_or(0.0)
        })
        .sum::<f64>();
    let final_score = weighted_sum.clamp(0.0, 100.0);

    (
        final_score,
        json!({
            "formula": "final = ranking baseline + metadata contributions + centered on-chain adjustment + sample utility + sample verified bonus",
            "raw_final_score": valuation.final_score,
            "coarse_score": valuation.coarse_score,
            "has_sample_score": valuation.proxy_utility.is_some(),
            "base_score": metadata_base,
            "components": components,
            "proxy_utility": valuation.proxy_utility.as_ref().map(|proxy_utility| json!({
                "utility_score": proxy_utility.utility_score,
                "apply_mode": proxy_utility.apply_mode,
                "proxy_metric_name": proxy_utility.proxy_metric_name,
                "proxy_metric_value": proxy_utility.proxy_metric_value,
                "sampled_rows": proxy_utility.sampled_rows,
                "sampled_bytes": proxy_utility.sampled_bytes,
                "notes": proxy_utility.notes,
            })),
        }),
    )
}

fn heuristic_verdict(score: f64) -> &'static str {
    match score {
        s if s > 60.0 => "strongpositive",
        s if s > 30.0 => "positive",
        s if s > 0.0 => "neutral",
        s if s > -30.0 => "negative",
        _ => "strongnegative",
    }
}

fn heuristic_report(result: &Value) -> Value {
    let columns = result
        .get("schema")
        .and_then(|schema| schema.get("columns"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let rows = result
        .get("schema")
        .and_then(|schema| schema.get("rows"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let reviews = result
        .get("community")
        .and_then(|community| community.get("total_reviews"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let positive_rate = result
        .get("community")
        .and_then(|community| community.get("positive_rate"))
        .map(extract_percentage)
        .unwrap_or(0.0);
    let negative_rate = result
        .get("community")
        .and_then(|community| community.get("negative_rate"))
        .map(extract_percentage)
        .unwrap_or(0.0);

    let size_score = ((rows.max(1) as f64) / 100.0).min(100.0);
    let schema_fit = if columns > 0 { 60.0 } else { 20.0 };
    let temporal_fit = 50.0;
    let information_gain = 60.0;
    let quality_score = (30.0 + size_score * 0.5).min(80.0);
    let community_signal = if reviews > 0 {
        positive_rate * 80.0
    } else {
        30.0
    };
    let risk_penalty = negative_rate * 100.0;
    let raw = 0.25 * schema_fit
        + 0.15 * temporal_fit
        + 0.15 * information_gain
        + 0.10 * quality_score
        + 0.15 * community_signal
        - 0.20 * risk_penalty;
    let tcv_score = raw.clamp(-100.0, 100.0);
    let verdict = heuristic_verdict(tcv_score);

    json!({
        "tcv": {
            "tcv_score": tcv_score,
            "schema_fit": schema_fit,
            "temporal_fit": temporal_fit,
            "information_gain": information_gain,
            "quality_score": quality_score,
            "community_signal": community_signal,
            "risk_penalty": risk_penalty,
            "verdict": verdict,
            "explanation": "Heuristic TCV fallback derived from the demo UI for externally discovered datasets."
        },
        "community_feedback": {
            "total_reviews": reviews,
            "avg_relevance": result
                .get("community")
                .and_then(|community| community.get("avg_relevance"))
                .map(extract_score)
                .unwrap_or(0.0),
            "positive_rate": positive_rate,
            "negative_rate": negative_rate,
            "task_specific_signals": Vec::<Value>::new(),
        }
    })
}

fn report_tcv_score(report: &Value) -> f64 {
    report
        .get("tcv")
        .and_then(|tcv| tcv.get("tcv_score"))
        .map(extract_score)
        .unwrap_or(0.0)
}

fn report_verdict(report: &Value) -> Value {
    report
        .get("tcv")
        .and_then(|tcv| tcv.get("verdict"))
        .cloned()
        .unwrap_or(Value::Null)
}

fn evaluation_has_final_value(result: &Value) -> bool {
    result
        .get("sample_scored")
        .and_then(|value| value.as_bool())
        .or_else(|| {
            result
                .get("final_score_breakdown")
                .and_then(|breakdown| breakdown.get("has_sample_score"))
                .and_then(|value| value.as_bool())
        })
        .unwrap_or(false)
}

fn evaluation_sort_score(result: &Value) -> f64 {
    let score = result
        .get("final_score")
        .map(extract_score)
        .filter(|score| score.is_finite())
        .unwrap_or_else(|| {
            result
                .get("tcv_score")
                .map(extract_score)
                .filter(|score| score.is_finite())
                .unwrap_or(0.0)
        });
    if evaluation_has_final_value(result) {
        score + 1000.0
    } else {
        score
    }
}

fn build_normalized_results(results: &[Value], profile: &QueryProfile) -> Vec<Value> {
    let standard = json!({
        "sample_unit": profile.data_standard.sample_unit,
        "canonical_columns": profile.data_standard.canonical_columns,
        "extra_columns": profile.data_standard.extra_columns,
    });

    results
        .iter()
        .map(|result| {
            json!({
                "dataset_id": result.get("cid").cloned().unwrap_or(Value::Null),
                "name": result.get("title").cloned().unwrap_or(Value::Null),
                "description": result.get("description").cloned().unwrap_or(Value::Null),
                "source": result.get("source").cloned().unwrap_or(Value::Null),
                "modality": result.get("data_type").cloned().unwrap_or(Value::Null),
                "price": result.get("price").cloned().unwrap_or(Value::Null),
                "observed_schema": result.get("schema").cloned().unwrap_or_else(|| json!({})),
                "target_standard": standard.clone(),
            })
        })
        .collect()
}

async fn search_results_json(
    state: &AppState,
    profile: &QueryProfile,
    filters: &SearchFilters,
    limit: usize,
) -> Result<(SearchOutput, Vec<Value>)> {
    let local_metadata = state.store.list_all()?;

    let fb_store = state.feedback_store.clone();
    let signal_fetcher: data_search::engine::SignalFetcher = Box::new(move |cid_str: &str| {
        let cid = DatasetCid(cid_str.to_string());
        fb_store
            .compute_signal(&cid)
            .unwrap_or_else(|_| CommunitySignal {
                dataset_cid: cid,
                total_reviews: 0,
                avg_relevance: 0.0,
                avg_quality: 0.0,
                positive_rate: 0.0,
                negative_rate: 0.0,
                task_signals: vec![],
            })
    });

    let search_output = state
        .search_engine
        .search_with_profile(profile, filters, &local_metadata, &signal_fetcher, limit)
        .await?;

    let results = search_output
        .results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            json!({
                "rank": i + 1,
                "cid": r.result.cid.0,
                "title": r.result.title,
                "description": r.result.description,
                "source": r.result.source,
                "data_type": r.result.data_type,
                "price": r.result.price,
                "schema": {
                    "columns": r.result.schema.columns.len(),
                    "rows": r.result.schema.row_count,
                    "size_bytes": r.result.schema.size_bytes,
                },
                "rank_score": format!("{:.1}", r.rank_score),
                "community": {
                    "total_reviews": r.signal.total_reviews,
                    "avg_relevance": format!("{:.2}", r.signal.avg_relevance),
                    "positive_rate": format!("{:.0}%", r.signal.positive_rate * 100.0),
                    "negative_rate": format!("{:.0}%", r.signal.negative_rate * 100.0),
                }
            })
        })
        .collect();

    Ok((search_output, results))
}

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
    let raw_query = args
        .get("raw_query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let intent_query = raw_query
        .or(query)
        .ok_or_else(|| anyhow::anyhow!("missing query"))?;
    let task_type_override = args
        .get("task_type")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let stop_after = parse_stop_after(&args)?;

    let search_args = args.get("search").cloned().unwrap_or_default();
    let evaluate_args = args.get("evaluate").cloned().unwrap_or_default();
    let (mut filters, limit) = collect_search_filters(&search_args);
    if matches!(state.tool_profile, ToolProfile::CodexWorkflow) {
        filters.skill_ids.clear();
    }
    let requested_top_k = evaluate_args
        .get("top_k")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let sampling_removed = true;
    let _ = sampling_removed;
    // Build profile from structured args if provided, otherwise use heuristic.
    let profile_result: Result<QueryProfile> = {
        let keywords: Vec<String> = args
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();
        let sample_unit = args.get("sample_unit").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let target_entity = args.get("target_entity").and_then(|v| v.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let budget = args.get("budget").and_then(|v| v.as_str()).unwrap_or("0 USD").trim().to_string();
        let task_desc = args.get("task_description").and_then(|v| v.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

        if keywords.is_empty() {
            // Fallback: extract keywords from query heuristically.
            let fallback_keywords: Vec<String> = intent_query
                .split_whitespace()
                .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
                .filter(|w| w.len() > 2)
                .take(5)
                .collect();
            Ok(QueryProfile {
                raw_query: intent_query.to_string(),
                task_type: task_type_override.clone(),
                task_description: task_desc.or_else(|| Some(intent_query.to_string())),
                target_entity,
                keywords: fallback_keywords,
                data_standard: data_search::intent::DataStandard {
                    sample_unit,
                    budget,
                    ..Default::default()
                },
                user_profile: Default::default(),
            })
        } else {
            Ok(QueryProfile {
                raw_query: intent_query.to_string(),
                task_type: task_type_override.clone().or_else(|| args.get("task_type").and_then(|v| v.as_str()).map(String::from)),
                task_description: task_desc.or_else(|| Some(intent_query.to_string())),
                target_entity,
                keywords,
                data_standard: data_search::intent::DataStandard {
                    sample_unit,
                    budget,
                    ..Default::default()
                },
                user_profile: Default::default(),
            })
        }
    };
    let mut profile = match profile_result {
        Ok(profile) => profile,
        Err(e) => {
            return Ok(serde_json::to_string_pretty(&json!({
                "status": "failed",
                "stop_after": stop_after.as_str(),
                "completed_steps": [],
                "failed_stage": StopAfter::IntentParse.as_str(),
                "error": e.to_string(),
            }))?)
        }
    };
    if let Some(task_type) = task_type_override {
        profile.task_type = Some(task_type);
    }

    if stop_after == StopAfter::IntentParse {
        return Ok(serde_json::to_string_pretty(&json!({
            "status": "completed",
            "stop_after": stop_after.as_str(),
            "completed_steps": [StopAfter::IntentParse.as_str()],
            "intent": compact_intent(&profile),
        }))?);
    }

    let (search_output, search_results) =
        match search_results_json(state, &profile, &filters, limit).await {
            Ok(result) => result,
            Err(e) => {
                return Ok(serde_json::to_string_pretty(&json!({
                    "status": "failed",
                    "stop_after": stop_after.as_str(),
                    "completed_steps": [StopAfter::IntentParse.as_str()],
                    "failed_stage": StopAfter::DatasetSearch.as_str(),
                    "error": e.to_string(),
                    "intent": compact_intent(&profile),
                }))?)
            }
        };
    let search_errors = search_output.errors.clone();
    let search_candidate_cids = extract_candidate_cids(&search_results);
    let compact_candidates: Vec<Value> = search_results.iter().map(compact_candidate).collect();

    if stop_after == StopAfter::DatasetSearch {
        if compact_candidates.is_empty() && !search_errors.is_empty() {
            return Ok(serde_json::to_string_pretty(&json!({
                "status": "failed",
                "stop_after": stop_after.as_str(),
                "completed_steps": [StopAfter::IntentParse.as_str()],
                "failed_stage": StopAfter::DatasetSearch.as_str(),
                "error": search_errors.first().cloned().unwrap_or_else(|| "dataset search failed".to_string()),
                "intent": compact_intent(&profile),
                "candidate_count": 0,
                "candidates": [],
            }))?);
        }

        return Ok(serde_json::to_string_pretty(&json!({
            "status": "completed",
            "stop_after": stop_after.as_str(),
            "completed_steps": [StopAfter::IntentParse.as_str(), StopAfter::DatasetSearch.as_str()],
            "intent": compact_intent(&profile),
            "candidate_count": compact_candidates.len(),
            "candidates": compact_candidates,
        }))?);
    }

    let _normalized_results = build_normalized_results(&search_results, &profile);

    if search_candidate_cids.is_empty() {
        let (status, failed_stage, error) = if search_errors.is_empty() {
            (
                "blocked",
                StopAfter::DatasetEvaluate.as_str(),
                "no candidate datasets available for evaluation".to_string(),
            )
        } else {
            (
                "failed",
                StopAfter::DatasetSearch.as_str(),
                search_errors
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "dataset search failed".to_string()),
            )
        };
        return Ok(serde_json::to_string_pretty(&json!({
            "status": status,
            "stop_after": stop_after.as_str(),
            "completed_steps": [StopAfter::IntentParse.as_str(), StopAfter::DatasetSearch.as_str()],
            "failed_stage": failed_stage,
            "error": error,
            "intent": compact_intent(&profile),
            "candidate_count": 0,
            "selected_dataset": Value::Null,
        }))?);
    }

    let required_columns = evaluate_required_columns(&evaluate_args, &profile);
    let budget = parse_budget_amount(evaluate_args.get("budget"))
        .or_else(|| parse_budget_amount_str(&profile.data_standard.budget))
        .unwrap_or(0.0);
    let task_description = profile
        .task_description
        .clone()
        .unwrap_or_else(|| profile.raw_query.clone());
    let task_type = profile
        .task_type
        .clone()
        .unwrap_or_else(|| "general".to_string());
    let evaluate_top_k = requested_top_k.unwrap_or(search_results.len());
    let local_metadata = state.store.list_all()?;
    let mut selection_task = DatasetSelectionTask::from(&profile);
    selection_task.task_description = task_description.clone();
    selection_task.task_type = task_type.clone();
    selection_task.max_total_price = Some(budget);
    if !required_columns.is_empty() {
        selection_task.required_columns = required_columns.clone();
    }
    let metadata_resolver = SearchResultMetadataResolver;
    let sample_evaluator = build_runtime_sample_evaluator();
    let selection_config = DatasetValuationConfig::default();
    let selection_output = state
        .search_engine
        .value_search_output(
            &search_output,
            &local_metadata,
            Some(&metadata_resolver),
            Some(&sample_evaluator),
            &selection_task,
            &selection_config,
        )
        .await?;
    let selected_collection = selection_output.selected_collection.clone();
    let selected_collection_cids: HashSet<String> = selected_collection
        .as_ref()
        .map(|collection| {
            collection
                .datasets
                .iter()
                .map(|dataset| dataset.result.cid.0.clone())
                .collect()
        })
        .unwrap_or_default();
    let selected_dataset_cid = selection_output
        .selected
        .as_ref()
        .map(|dataset| dataset.result.cid.0.clone());
    let mut valuation_by_cid: HashMap<String, DatasetValuation> = HashMap::new();
    for candidate in selection_output.candidates {
        valuation_by_cid.insert(candidate.result.cid.0.clone(), candidate);
    }
    let has_sample_scored_candidates = valuation_by_cid
        .values()
        .any(|candidate| candidate.proxy_utility.is_some());

    let mut evaluated_results = Vec::new();
    let mut stage_errors = selection_output.errors;

    for (index, result) in search_results.iter().enumerate() {
        let cid = result
            .get("cid")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        if cid.is_empty() {
            continue;
        }
        if index >= evaluate_top_k && !selected_collection_cids.contains(&cid) {
            continue;
        }

        let dataset_cid = DatasetCid(cid.clone());
        let report = if state.store.get(&dataset_cid)?.is_some() {
            match crate::handlers::evaluate::handle(
                json!({
                    "cid": cid,
                    "task_description": task_description,
                    "task_type": task_type,
                    "required_columns": required_columns,
                    "budget": budget,
                }),
                state,
            )
            .await
            {
                Ok(report) => parse_json_or_text(&report),
                Err(e) => {
                    stage_errors.push(e.to_string());
                    continue;
                }
            }
        } else {
            heuristic_report(result)
        };

        let valuation = valuation_by_cid.get(&cid);
        let (final_score, final_score_breakdown) = valuation
            .map(|candidate| {
                final_score_from_valuation(
                    candidate,
                    &selection_config,
                    has_sample_scored_candidates,
                )
            })
            .unwrap_or_else(|| {
                let fallback = report_tcv_score(&report);
                (
                    fallback,
                    json!({
                        "formula": "final = fallback tcv score",
                        "raw_final_score": fallback,
                        "coarse_score": fallback,
                        "has_sample_score": false,
                        "components": [{
                            "id": "fallback_tcv",
                            "label": "Fallback TCV",
                            "value": fallback,
                            "contribution": fallback,
                        }],
                        "proxy_utility": Value::Null,
                    }),
                )
            });
        let coarse_score = valuation
            .map(|candidate| candidate.coarse_score)
            .unwrap_or(final_score);
        let on_chain_score = valuation
            .map(|candidate| candidate.on_chain_score)
            .unwrap_or(ON_CHAIN_SCORE_NEUTRAL);
        let raw_final_score = valuation
            .map(|candidate| candidate.final_score)
            .unwrap_or(final_score);

        evaluated_results.push(json!({
            "cid": cid,
            "title": result.get("title").cloned().unwrap_or(Value::Null),
            "description": result.get("description").cloned().unwrap_or(Value::Null),
            "source": result.get("source").cloned().unwrap_or(Value::Null),
            "data_type": result.get("data_type").cloned().unwrap_or(Value::Null),
            "price": result.get("price").cloned().unwrap_or(Value::Null),
            "schema": result.get("schema").cloned().unwrap_or_else(|| json!({})),
            "evaluation_mode": "selection_pipeline",
            "coarse_score": coarse_score,
            "on_chain_score": on_chain_score,
            "raw_final_score": raw_final_score,
            "final_score": final_score,
            "final_score_breakdown": final_score_breakdown,
            "tcv_score": report_tcv_score(&report),
            "tcv": report.get("tcv").cloned().unwrap_or(Value::Null),
            "community_feedback": report
                .get("community_feedback")
                .cloned()
                .unwrap_or(Value::Null),
            "selection_explanation": valuation
                .map(|candidate| candidate.explanation.clone())
                .unwrap_or_default(),
            "sample_scored": valuation
                .map(|candidate| candidate.proxy_utility.is_some())
                .unwrap_or(false),
            "sample_failure_reason": valuation
                .and_then(|candidate| candidate.sample_failure_reason.clone())
                .unwrap_or_default(),
            "selected_in_collection": selected_collection_cids.contains(&cid),
            "verdict": report_verdict(&report),
        }));
    }

    evaluated_results.sort_by(|left, right| {
        let left_selected = left
            .get("selected_in_collection")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let right_selected = right
            .get("selected_in_collection")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if left_selected != right_selected {
            return right_selected.cmp(&left_selected);
        }
        let left_score = evaluation_sort_score(left);
        let right_score = evaluation_sort_score(right);
        right_score
            .partial_cmp(&left_score)
            .unwrap_or(Ordering::Equal)
    });

    if evaluated_results.is_empty() {
        return Ok(serde_json::to_string_pretty(&json!({
            "status": "failed",
            "stop_after": stop_after.as_str(),
            "completed_steps": [StopAfter::IntentParse.as_str(), StopAfter::DatasetSearch.as_str()],
            "failed_stage": StopAfter::DatasetEvaluate.as_str(),
            "error": stage_errors
                .first()
                .cloned()
                .unwrap_or_else(|| "dataset evaluation produced no usable result".to_string()),
            "intent": compact_intent(&profile),
            "candidate_count": compact_candidates.len(),
            "selected_dataset": Value::Null,
        }))?);
    }

    let selected_dataset = selected_dataset_cid
        .as_deref()
        .and_then(|cid| {
            evaluated_results
                .iter()
                .find(|result| result.get("cid").and_then(Value::as_str) == Some(cid))
        })
        .map(compact_selected_dataset)
        .or_else(|| evaluated_results.first().map(compact_selected_dataset))
        .unwrap_or(Value::Null);
    let selected_datasets: Vec<Value> = evaluated_results
        .iter()
        .filter(|result| {
            result
                .get("selected_in_collection")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .map(compact_selected_dataset)
        .collect();
    let selection_summary = selected_collection
        .map(|collection| {
            json!({
                "selected_count": collection.datasets.len(),
                "total_price": collection.total_price,
                "total_size_bytes": collection.total_size_bytes,
                "total_value": collection.total_value,
                "batch_index": collection.batch_index,
                "batch_start_rank": collection.batch_start_rank,
                "batch_end_rank": collection.batch_end_rank,
                "price_step": collection.price_step,
                "objective": "maximize sum(row_count * estimated_average_value_score)",
            })
        })
        .unwrap_or(Value::Null);

    Ok(serde_json::to_string_pretty(&json!({
        "status": "completed",
        "stop_after": stop_after.as_str(),
        "completed_steps": [
            StopAfter::IntentParse.as_str(),
            StopAfter::DatasetSearch.as_str(),
            StopAfter::DatasetEvaluate.as_str(),
        ],
        "intent": compact_intent(&profile),
        "candidate_count": compact_candidates.len(),
        "selected_dataset": selected_dataset,
        "selected_datasets": selected_datasets,
        "selection_summary": selection_summary,
        "evaluated_candidates": evaluated_results,
        "errors": stage_errors,
    }))?)
}

#[cfg(test)]
mod tests {
    use super::{
        compact_intent, evaluation_has_final_value, evaluation_sort_score, extract_percentage,
        final_score_from_valuation, heuristic_report, parse_stop_after,
        sample_contains_image_records, StopAfter,
    };
    use chrono::Utc;
    use data_core::types::{
        DataSource, DataType, DatasetSchema, Did, License, Price, SearchResult,
    };
    use data_search::engine::{
        DatasetValuation, ProxyUtilityApplyMode, ProxyUtilityReport, ON_CHAIN_COARSE_ADJUST_WEIGHT,
        ON_CHAIN_SCORE_NEUTRAL,
    };
    use data_search::intent::QueryProfile;
    use data_search::sample_eval::{DownloadedSample, SampleRecord};
    use serde_json::json;

    #[test]
    fn heuristic_report_matches_demo_ui_defaults_without_reviews() {
        let report = heuristic_report(&json!({
            "community": {
                "total_reviews": 0,
                "avg_relevance": "0.00",
                "positive_rate": "0%",
                "negative_rate": "0%"
            },
            "schema": {
                "columns": 0,
                "rows": 0,
                "size_bytes": 1024
            }
        }));

        let score = report["tcv"]["tcv_score"].as_f64().unwrap();
        assert!(
            (score - 29.0005).abs() < 1e-6,
            "unexpected heuristic score: {score}"
        );
        assert_eq!(report["tcv"]["verdict"], "neutral");
    }

    #[test]
    fn extract_percentage_parses_percent_strings() {
        let value = json!("75%");
        let parsed = extract_percentage(&value);
        assert!((parsed - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn evaluation_sort_score_prefers_final_score_when_present() {
        let candidate = json!({
            "final_score": 87.5,
            "tcv_score": 64.0,
        });
        assert!((evaluation_sort_score(&candidate) - 87.5).abs() < f64::EPSILON);

        let fallback = json!({
            "tcv_score": 64.0,
        });
        assert!((evaluation_sort_score(&fallback) - 64.0).abs() < f64::EPSILON);
    }

    #[test]
    fn evaluation_has_final_value_detects_sample_scored_candidates() {
        assert!(evaluation_has_final_value(&json!({
            "sample_scored": true,
            "final_score": 72.0,
        })));
        assert!(!evaluation_has_final_value(&json!({
            "sample_scored": false,
            "final_score": 88.0,
        })));
    }

    #[test]
    fn evaluation_sort_score_ranks_sample_scored_candidates_above_unsampled_ones() {
        let sampled = json!({
            "sample_scored": true,
            "final_score": 70.0,
        });
        let unsampled = json!({
            "sample_scored": false,
            "final_score": 95.0,
        });

        assert!(evaluation_sort_score(&sampled) > evaluation_sort_score(&unsampled));
    }

    #[test]
    fn sampled_candidates_receive_higher_display_scores_than_unsampled_candidates() {
        fn fake_result(cid: &str) -> SearchResult {
            SearchResult {
                cid: data_core::types::DatasetCid(cid.into()),
                title: cid.into(),
                description: None,
                tags: vec![],
                schema: DatasetSchema {
                    columns: vec![],
                    row_count: 0,
                    size_bytes: 0,
                },
                quality: None,
                price: Price::free(),
                license: License {
                    spdx_id: "CC-BY-4.0".into(),
                    commercial_use: true,
                    derivative_allowed: true,
                },
                provider: Did("did:test".into()),
                source: DataSource::GuixuHub,
                market: None,
                data_type: DataType::Tabular,
                created_at: Utc::now(),
                seller_endpoint: None,
                source_attributes: None,
            }
        }

        let unsampled = DatasetValuation {
            result: fake_result("unsampled"),
            coarse_score: 100.0,
            final_score: 100.0,
            relevance: 100.0,
            schema_fit: 100.0,
            scale_score: 100.0,
            label_quality: 100.0,
            metadata_completeness: 100.0,
            on_chain_score: 50.0,
            metadata_resolved: false,
            sample_plan: None,
            proxy_utility: None,
            sample_failure_reason: None,
            explanation: String::new(),
        };
        let sampled = DatasetValuation {
            result: fake_result("sampled"),
            coarse_score: 10.0,
            final_score: 10.0,
            relevance: 10.0,
            schema_fit: 10.0,
            scale_score: 10.0,
            label_quality: 10.0,
            metadata_completeness: 10.0,
            on_chain_score: 50.0,
            metadata_resolved: true,
            sample_plan: None,
            proxy_utility: Some(ProxyUtilityReport {
                utility_score: 10.0,
                apply_mode: ProxyUtilityApplyMode::Blend,
                proxy_metric_name: "sample_similarity".into(),
                proxy_metric_value: 10.0,
                sampled_rows: 4,
                sampled_bytes: 1024,
                notes: None,
            }),
            sample_failure_reason: None,
            explanation: String::new(),
        };

        let config = data_search::engine::DatasetValuationConfig::default();
        let (unsampled_score, _) = final_score_from_valuation(&unsampled, &config, true);
        let (sampled_score, _) = final_score_from_valuation(&sampled, &config, true);
        assert!(sampled_score > unsampled_score);
        assert!(sampled_score >= 72.0);
        assert!(unsampled_score >= 45.0);
    }

    #[test]
    fn final_score_breakdown_reports_centered_on_chain_component() {
        let valuation = DatasetValuation {
            result: SearchResult {
                cid: data_core::types::DatasetCid("guixu-hub:test".into()),
                title: "test".into(),
                description: None,
                tags: vec![],
                schema: DatasetSchema {
                    columns: vec![],
                    row_count: 0,
                    size_bytes: 0,
                },
                quality: None,
                price: Price::free(),
                license: License {
                    spdx_id: "CC-BY-4.0".into(),
                    commercial_use: true,
                    derivative_allowed: true,
                },
                provider: Did("guixu:hub:test".into()),
                source: DataSource::GuixuHub,
                market: None,
                data_type: DataType::Tabular,
                created_at: Utc::now(),
                seller_endpoint: None,
                source_attributes: None,
            },
            coarse_score: 80.0,
            final_score: 80.0,
            relevance: 80.0,
            schema_fit: 80.0,
            scale_score: 80.0,
            label_quality: 80.0,
            metadata_completeness: 80.0,
            on_chain_score: 90.0,
            metadata_resolved: true,
            sample_plan: None,
            proxy_utility: None,
            sample_failure_reason: None,
            explanation: String::new(),
        };

        let config = data_search::engine::DatasetValuationConfig::default();
        let (_, breakdown) = final_score_from_valuation(&valuation, &config, false);
        let on_chain_component = breakdown["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|component| component["id"] == "on_chain_score")
            .cloned()
            .unwrap();
        let expected = 0.35 * ON_CHAIN_COARSE_ADJUST_WEIGHT * (90.0 - ON_CHAIN_SCORE_NEUTRAL);

        assert_eq!(on_chain_component["value"], 90.0);
        assert_eq!(on_chain_component["baseline"], ON_CHAIN_SCORE_NEUTRAL);
        assert!((on_chain_component["contribution"].as_f64().unwrap() - expected).abs() < 1e-6);
    }

    #[test]
    fn parse_stop_after_defaults_to_dataset_evaluate() {
        let stop_after = parse_stop_after(&json!({})).unwrap();
        assert_eq!(stop_after, StopAfter::DatasetEvaluate);
    }

    #[test]
    fn sample_contains_image_records_detects_local_image_metadata() {
        let sample = DownloadedSample {
            records: vec![SampleRecord {
                id: "sample-01".into(),
                content: "image sample".into(),
                metadata: json!({
                    "kind": "image",
                    "local_image_path": "/tmp/sample-01.jpg",
                    "local_image_mime_type": "image/jpeg",
                }),
            }],
            sampled_rows: 1,
            sampled_bytes: 1024,
            summary: Some("image sample".into()),
        };

        assert!(sample_contains_image_records(&sample));
    }

    #[test]
    fn parse_stop_after_supports_legacy_pipeline_flags() {
        let stop_after = parse_stop_after(&json!({
            "pipeline": {
                "intent_parse": true,
                "dataset_search": true,
                "dataset_evaluate": false
            }
        }))
        .unwrap();
        assert_eq!(stop_after, StopAfter::DatasetSearch);
    }

    #[test]
    fn compact_intent_includes_transfer_constraints() {
        let profile = QueryProfile {
            raw_query: "find cat dataset".into(),
            data_standard: data_search::intent::DataStandard {
                budget: "$20".into(),
                max_latency_secs: 45.0,
                min_dataset_size_bytes: 500_000_000,
                max_dataset_size_bytes: 562_500_000,
                ..Default::default()
            },
            ..Default::default()
        };

        let compact = compact_intent(&profile);
        assert_eq!(compact["data_standard"]["budget"], "$20");
        assert_eq!(compact["data_standard"]["max_latency_secs"], 45.0);
        assert_eq!(
            compact["data_standard"]["min_dataset_size_bytes"],
            500_000_000
        );
        assert_eq!(
            compact["data_standard"]["max_dataset_size_bytes"],
            562_500_000
        );
    }
}
  562_500_000
        );
    }
}
0_000
        );
    }
}
0_000
        );
    }
}
