use chrono::Utc;
use data_core::feedback::CommunitySignal;
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;
use serde::Serialize;
use std::collections::HashMap;

use crate::adapters::ExternalAdapter;
use crate::engine::{
    DatasetSelectionTask, DatasetValuationConfig, MetadataResolver, ProxyUtilityReport,
    SampleEvaluator, SamplePlan, SearchEngine, SearchFilters, SignalFetcher,
};
use crate::intent::{IntentParser, QueryProfile, QueryProfiler};
use crate::vector_index::VectorIndex;

fn dump_json<T: Serialize>(label: &str, value: &T) {
    let json = serde_json::to_string_pretty(value).unwrap();
    println!("{label}:\n{json}");
}

fn dump_profile_fields(profile: &QueryProfile) {
    println!(
        "profile.fields: task_type={:?}, target_entity={:?}, quality_hint={:?}, keywords={:?}",
        profile.task_type, profile.target_entity, profile.quality_hint, profile.keywords
    );
}

fn make_metadata_with_shape(
    cid_suffix: &str,
    title: &str,
    description: &str,
    tags: &[&str],
    row_count: u64,
    size_bytes: u64,
    null_rate: Option<f64>,
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
            row_count,
            size_bytes,
        },
        stats: null_rate.map(|rate| DatasetStats {
            null_rate: rate,
            unique_rate: 0.5,
            min_values: serde_json::json!({}),
            max_values: serde_json::json!({}),
        }),
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

fn make_metadata(
    cid_suffix: &str,
    title: &str,
    description: &str,
    tags: &[&str],
) -> DatasetMetadata {
    make_metadata_with_shape(cid_suffix, title, description, tags, 100, 2_048, None)
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
        created_at: Utc::now(),
    }
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

struct StubMetadataResolver {
    metadata_by_cid: HashMap<String, DatasetMetadata>,
}

#[async_trait::async_trait]
impl MetadataResolver for StubMetadataResolver {
    async fn resolve_metadata(
        &self,
        result: &SearchResult,
    ) -> anyhow::Result<Option<DatasetMetadata>> {
        Ok(self.metadata_by_cid.get(&result.cid.0).cloned())
    }
}

struct StubSampleEvaluator {
    utility_by_cid: HashMap<String, f64>,
}

#[async_trait::async_trait]
impl SampleEvaluator for StubSampleEvaluator {
    async fn evaluate_sample(
        &self,
        metadata: &DatasetMetadata,
        _task: &DatasetSelectionTask,
        plan: &SamplePlan,
    ) -> anyhow::Result<Option<ProxyUtilityReport>> {
        Ok(self
            .utility_by_cid
            .get(&metadata.cid.0)
            .copied()
            .map(|utility_score| ProxyUtilityReport {
                utility_score,
                proxy_metric_name: "proxy_f1".into(),
                proxy_metric_value: utility_score / 100.0,
                sampled_rows: plan.estimated_rows,
                sampled_bytes: plan.estimated_bytes,
                notes: None,
            }))
    }
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
    SearchEngine::new(VectorIndex, IntentParser, adapters)
}

#[tokio::test]
async fn intent_parser_profiles_raw_query_and_keywords() {
    let parser = IntentParser;
    let profile = parser
        .profile("Build a high-quality classifier to detect cats")
        .await
        .unwrap();

    dump_json("profile", &profile);
    dump_profile_fields(&profile);

    assert_eq!(
        profile.raw_query,
        "Build a high-quality classifier to detect cats"
    );
    assert_eq!(profile.task_type.as_deref(), Some("classification"));
    assert_eq!(profile.target_entity.as_deref(), Some("cats"));
    assert_eq!(profile.quality_hint.as_deref(), Some("high-quality"));
    assert_eq!(
        profile.keywords,
        vec!["build", "high-quality", "classifier", "detect", "cats"]
    );
}

#[tokio::test]
async fn intent_parser_trait_returns_same_profile_as_inherent_method() {
    let parser = IntentParser;
    let profiler: &dyn QueryProfiler = &parser;

    let via_inherent = parser
        .profile("Build a high-quality classifier to detect cats")
        .await
        .unwrap();
    let via_trait = profiler
        .profile("Build a high-quality classifier to detect cats")
        .await
        .unwrap();

    assert_eq!(via_inherent, via_trait);
}

#[tokio::test]
async fn search_with_profile_matches_local_metadata() {
    let engine = make_engine(vec![]);
    let local_metadata = vec![
        make_metadata(
            "cats",
            "Cat Image Classification Dataset",
            "Labeled cat images for image classification",
            &["cats", "classification", "images"],
        ),
        make_metadata(
            "dogs",
            "Dog Image Dataset",
            "Labeled dog images for classification",
            &["dogs", "classification", "images"],
        ),
    ];
    let profile = QueryProfile {
        raw_query: "Build a high-quality classifier to detect cats".into(),
        task_type: Some("classification".into()),
        target_entity: Some("cats".into()),
        quality_hint: Some("high-quality".into()),
        keywords: vec![
            "build".into(),
            "high-quality".into(),
            "classifier".into(),
            "detect".into(),
            "cats".into(),
        ],
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
        target_entity: Some("cats".into()),
        quality_hint: Some("high-quality".into()),
        keywords: vec!["cats".into(), "classifier".into()],
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
async fn search_wrapper_preserves_existing_behaviour_by_profiling_then_searching() {
    let engine = make_engine(vec![]);
    let local_metadata = vec![make_metadata(
        "cats",
        "Cat Image Classification Dataset",
        "Labeled cat images for image classification",
        &["cats", "classification", "images"],
    )];
    let filters = SearchFilters::default();
    let profile = QueryProfile {
        raw_query: "Build a high-quality classifier to detect cats".into(),
        task_type: Some("classification".into()),
        target_entity: Some("cats".into()),
        quality_hint: Some("high-quality".into()),
        keywords: vec![
            "build".into(),
            "high-quality".into(),
            "classifier".into(),
            "detect".into(),
            "cats".into(),
        ],
    };

    let via_wrapper = engine
        .search(
            "Build a high-quality classifier to detect cats",
            &filters,
            &local_metadata,
            &neutral_signal_fetcher(),
            10,
        )
        .await
        .unwrap();
    let via_profile = engine
        .search_with_profile(
            &profile,
            &filters,
            &local_metadata,
            &neutral_signal_fetcher(),
            10,
        )
        .await
        .unwrap();

    dump_json("wrapper.results", &via_wrapper.results);
    dump_json("profile.results", &via_profile.results);

    assert_eq!(via_wrapper.results.len(), via_profile.results.len());
    assert_eq!(
        via_wrapper.results[0].result.cid.0,
        via_profile.results[0].result.cid.0
    );
}

#[tokio::test]
async fn search_and_value_prefers_relevant_dataset_before_sampling() {
    let engine = make_engine(vec![]);
    let local_metadata = vec![
        make_metadata_with_shape(
            "cats-balanced",
            "Balanced Cat Image Classification Dataset",
            "Balanced cat images for classifier training with labels",
            &["cats", "classification", "balanced"],
            25_000,
            80 * 1024 * 1024,
            Some(0.02),
        ),
        make_metadata_with_shape(
            "dogs-large",
            "Dog Image Classification Dataset",
            "Large dog images for classifier training",
            &["dogs", "classification", "images"],
            250_000,
            640 * 1024 * 1024,
            Some(0.02),
        ),
        make_metadata_with_shape(
            "cats-tiny",
            "Tiny Cat Classification Sample",
            "Small cat classification sample",
            &["cats", "classification"],
            120,
            512 * 1024,
            Some(0.01),
        ),
    ];
    let config = DatasetValuationConfig {
        coarse_top_k: 0,
        ..DatasetValuationConfig::default()
    };

    let output = engine
        .search_and_value(
            "Build a high-quality classifier to detect cats",
            &SearchFilters::default(),
            &local_metadata,
            &neutral_signal_fetcher(),
            None,
            None,
            None,
            &config,
            10,
        )
        .await
        .unwrap();

    dump_json("search.value.coarse", &output);

    let selected = output.selected.as_ref().unwrap();
    assert_eq!(selected.result.cid.0, "cid-cats-balanced");
    assert!(selected.proxy_utility.is_none());
    assert!(selected.final_score >= output.candidates[1].final_score);
}

#[tokio::test]
async fn search_and_value_uses_proxy_utility_to_rerank_top_k() {
    let engine = make_engine(vec![]);
    let local_metadata = vec![
        make_metadata_with_shape(
            "cats-large",
            "Cat Image Classification Dataset",
            "Large cat images for classification",
            &["cats", "classification"],
            300_000,
            900 * 1024 * 1024,
            Some(0.03),
        ),
        make_metadata_with_shape(
            "cats-clean",
            "Curated Cat Image Classification Dataset",
            "Cat images for classification",
            &["cats", "classification"],
            5_000,
            24 * 1024 * 1024,
            Some(0.01),
        ),
    ];
    let sample_evaluator = StubSampleEvaluator {
        utility_by_cid: HashMap::from([
            ("cid-cats-large".to_string(), 35.0),
            ("cid-cats-clean".to_string(), 95.0),
        ]),
    };
    let config = DatasetValuationConfig {
        coarse_top_k: 2,
        metadata_weight: 0.6,
        utility_weight: 0.4,
        ..DatasetValuationConfig::default()
    };

    let output = engine
        .search_and_value(
            "Build a high-quality classifier to detect cats",
            &SearchFilters::default(),
            &local_metadata,
            &neutral_signal_fetcher(),
            None,
            Some(&sample_evaluator),
            None,
            &config,
            10,
        )
        .await
        .unwrap();

    dump_json("search.value.proxy", &output);

    let large = output
        .candidates
        .iter()
        .find(|candidate| candidate.result.cid.0 == "cid-cats-large")
        .unwrap();
    let clean = output
        .candidates
        .iter()
        .find(|candidate| candidate.result.cid.0 == "cid-cats-clean")
        .unwrap();

    assert!(large.coarse_score > clean.coarse_score);
    assert!(clean.final_score > large.final_score);
    assert_eq!(
        output.selected.as_ref().unwrap().result.cid.0,
        "cid-cats-clean"
    );
    assert!(clean.proxy_utility.is_some());
}

#[tokio::test]
async fn search_and_value_resolves_external_metadata_before_sampling() {
    let engine = make_engine(vec![Box::new(StubAdapter {
        results: vec![make_external_result(
            "cats-external",
            "Cat Image Classification Dataset",
            "Cat images from an external dataset hub",
        )],
    })]);
    let resolver = StubMetadataResolver {
        metadata_by_cid: HashMap::from([(
            "cid-cats-external".to_string(),
            make_metadata_with_shape(
                "cats-external",
                "Cat Image Classification Dataset",
                "Balanced cat images for classifier training with labels",
                &["cats", "classification", "balanced"],
                40_000,
                120 * 1024 * 1024,
                Some(0.01),
            ),
        )]),
    };
    let sample_evaluator = StubSampleEvaluator {
        utility_by_cid: HashMap::from([("cid-cats-external".to_string(), 88.0)]),
    };
    let config = DatasetValuationConfig {
        coarse_top_k: 1,
        ..DatasetValuationConfig::default()
    };

    let output = engine
        .search_and_value(
            "Build a high-quality classifier to detect cats",
            &SearchFilters::default(),
            &[],
            &neutral_signal_fetcher(),
            Some(&resolver),
            Some(&sample_evaluator),
            None,
            &config,
            10,
        )
        .await
        .unwrap();

    dump_json("search.value.external", &output);

    let selected = output.selected.as_ref().unwrap();
    assert_eq!(selected.result.cid.0, "cid-cats-external");
    assert!(selected.metadata_resolved);
    assert!(selected.sample_plan.is_some());
    assert!(selected.proxy_utility.is_some());
    assert!(output.errors.is_empty());
}
