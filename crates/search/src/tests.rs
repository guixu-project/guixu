use chrono::Utc;
use data_core::feedback::CommunitySignal;
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;
use serde::Serialize;
use std::collections::HashMap;

use crate::adapters::{self, ExternalAdapter};
use crate::engine::{
    DatasetSelectionTask, DatasetValuationConfig, MetadataResolver, ProxyUtilityReport,
    SampleEvaluator, SamplePlan, SearchEngine, SearchFilters, SignalFetcher,
};
use crate::intent::{
    retrieve_related_memories_for_test, IntentParser, IntentParserConfig, QueryProfile,
    QueryProfiler, UserProfile,
};
use crate::vector_index::VectorIndex;

fn dump_json<T: Serialize>(label: &str, value: &T) {
    let json = serde_json::to_string_pretty(value).unwrap();
    println!("{label}:\n{json}");
}

fn dump_profile_fields(profile: &QueryProfile) {
    println!(
        "profile.fields: task_type={:?}, task_description={:?}, target_entity={:?}, quality_hint={:?}, keywords={:?}, user_profile={:?}",
        profile.task_type,
        profile.task_description,
        profile.target_entity,
        profile.quality_hint,
        profile.keywords,
        profile.user_profile
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
    SearchEngine::new(
        VectorIndex,
        IntentParser::new(IntentParserConfig {
            api_key: None,
            ..IntentParserConfig::default()
        }),
        adapters,
    )
}

#[tokio::test]
async fn intent_parser_profiles_raw_query_and_keywords() {
    let parser = IntentParser::new(IntentParserConfig {
        api_key: None,
        ..IntentParserConfig::default()
    });
    let query = "Build a high-quality classifier to detect cats";
    let profile = parser.profile(query).await.unwrap();

    dump_json("profile", &profile);
    dump_profile_fields(&profile);

    assert_eq!(profile.raw_query, query);
    assert_eq!(profile.task_type.as_deref(), Some("classification"));
    assert_eq!(
        profile.task_description.as_deref(),
        Some(
            "Perform a classification task focused on cats with a high-quality requirement based on the user request: Build a high-quality classifier to detect cats"
        )
    );
    assert_eq!(profile.target_entity.as_deref(), Some("cats"));
    assert_eq!(profile.quality_hint.as_deref(), Some("high-quality"));
    assert_eq!(
        profile.keywords,
        vec!["build", "high-quality", "classifier", "detect", "cats"]
    );
    assert_eq!(
        profile.user_profile.cpu.architecture,
        std::env::consts::ARCH
    );
    assert!(profile.user_profile.cpu.logical_cores >= 1);
}

#[tokio::test]
async fn intent_parser_trait_returns_same_profile_as_inherent_method() {
    let parser = IntentParser::new(IntentParserConfig {
        api_key: None,
        ..IntentParserConfig::default()
    });
    let profiler: &dyn QueryProfiler = &parser;
    let query = "Build a high-quality classifier to detect cats";

    let via_inherent = parser.profile(query).await.unwrap();
    let via_trait = profiler.profile(query).await.unwrap();

    assert_eq!(via_inherent, via_trait);
}

#[tokio::test]
async fn intent_parser_uses_deepseek_when_configured() {
    let query = "check whether little Wu is in the image taken from monitor";
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
                "The user has a cat named little Wu.".to_string(),
                "The user prefers calm spaces more than loud ones.".to_string(),
            ],
        )
        .unwrap();
    let profile = parser
        .profile_from_deepseek_content(
            query,
            &user_profile,
            r#"{"task_type":"classification","task_description":"Detect whether cats are present in input images with high-quality accuracy.","target_entity":"cats","quality_hint":"high-quality","keywords":["cats","classifier","vision"]}"#,
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
        .contains("little Wu"));
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
    assert_eq!(profile.quality_hint.as_deref(), Some("high-quality"));
    assert_eq!(profile.keywords, vec!["cats", "classifier", "vision"]);
    assert_eq!(profile.user_profile, user_profile);
}

#[test]
fn memory_search_prefers_entries_matching_named_entities_and_terms() {
    let matches = retrieve_related_memories_for_test(
        "Plan a calm weekend around little Wu with small gatherings",
        &[
            "The user has a cat named little Wu.",
            "The user prefers small gatherings to crowded events.",
            "The user likes calm spaces more than loud energetic ones.",
            "The user enjoys cycling on cool mornings.",
        ],
        3,
    );

    dump_json("memory.matches", &matches);

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0], "The user has a cat named little Wu.");
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
        task_description: Some(
            "Perform a classification task focused on cats with a high-quality requirement based on the user request: Build a high-quality classifier to detect cats"
                .into(),
        ),
        target_entity: Some("cats".into()),
        quality_hint: Some("high-quality".into()),
        keywords: vec![
            "build".into(),
            "high-quality".into(),
            "classifier".into(),
            "detect".into(),
            "cats".into(),
        ],
        user_profile: UserProfile::default(),
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
        task_description: Some(
            "Perform a classification task focused on cats with a high-quality requirement based on the user request: Build a high-quality classifier to detect cats"
                .into(),
        ),
        target_entity: Some("cats".into()),
        quality_hint: Some("high-quality".into()),
        keywords: vec!["cats".into(), "classifier".into()],
        user_profile: UserProfile::default(),
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
        task_description: Some(
            "Perform a classification task focused on cats with a high-quality requirement based on the user request: Build a high-quality classifier to detect cats"
                .into(),
        ),
        target_entity: Some("cats".into()),
        quality_hint: Some("high-quality".into()),
        keywords: vec![
            "build".into(),
            "high-quality".into(),
            "classifier".into(),
            "detect".into(),
            "cats".into(),
        ],
        user_profile: UserProfile::default(),
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
async fn search_and_value_discards_candidates_outside_coarse_top_k() {
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
        make_metadata_with_shape(
            "cats-balanced",
            "Balanced Cat Image Classification Dataset",
            "Balanced cat images for classifier training with labels",
            &["cats", "classification", "balanced"],
            25_000,
            80 * 1024 * 1024,
            Some(0.02),
        ),
    ];
    let sample_evaluator = StubSampleEvaluator {
        utility_by_cid: HashMap::from([
            ("cid-cats-large".to_string(), 35.0),
            ("cid-cats-clean".to_string(), 95.0),
            ("cid-cats-balanced".to_string(), 100.0),
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

    assert_eq!(output.candidates.len(), 3);
    let clean = output
        .candidates
        .iter()
        .find(|candidate| candidate.result.cid.0 == "cid-cats-clean")
        .unwrap();
    assert!(clean.sample_plan.is_none());
    assert!(clean.proxy_utility.is_none());
    assert!((clean.final_score - clean.coarse_score).abs() < f64::EPSILON);
    assert_eq!(
        output.selected.as_ref().unwrap().result.cid.0,
        "cid-cats-balanced"
    );
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

#[tokio::test]
async fn search_and_value_skips_sampling_when_row_count_is_zero_even_if_bytes_exist() {
    let engine = make_engine(vec![]);
    let local_metadata = vec![make_metadata_with_shape(
        "cats-image-file",
        "Cat Image File",
        "Single cat image example",
        &["cats", "image"],
        0,
        2 * 1024 * 1024,
        None,
    )];
    let sample_evaluator = StubSampleEvaluator {
        utility_by_cid: HashMap::from([("cid-cats-image-file".to_string(), 91.0)]),
    };
    let config = DatasetValuationConfig {
        coarse_top_k: 1,
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

    let selected = output.selected.as_ref().unwrap();
    assert_eq!(selected.result.cid.0, "cid-cats-image-file");
    assert!(selected.sample_plan.is_none());
    assert!(selected.proxy_utility.is_none());
}

#[tokio::test]
async fn search_and_value_scores_cat_home_candidates() {
    let engine = make_engine(vec![]);
    let local_metadata = vec![
        make_metadata_with_shape(
            "cat-home-indoor",
            "Indoor Cat Home Images",
            "Labeled cat images in home indoor scenes for classification",
            &["cat", "home", "indoor", "classification"],
            18_000,
            64 * 1024 * 1024,
            Some(0.01),
        ),
        make_metadata_with_shape(
            "cat-outdoor",
            "Outdoor Cat Images",
            "Labeled cat images in outdoor streets and gardens for classification",
            &["cat", "outdoor", "classification"],
            19_000,
            66 * 1024 * 1024,
            Some(0.02),
        ),
        make_metadata_with_shape(
            "home-interior",
            "Home Interior Room Images",
            "Indoor home room images with furniture labels",
            &["home", "indoor", "rooms"],
            17_000,
            60 * 1024 * 1024,
            Some(0.03),
        ),
        make_metadata_with_shape(
            "dog-home",
            "Dog Home Companion Images",
            "Labeled dog images in home indoor scenes for classification",
            &["dog", "home", "classification"],
            16_000,
            62 * 1024 * 1024,
            Some(0.02),
        ),
    ];
    let sample_evaluator = StubSampleEvaluator {
        utility_by_cid: HashMap::from([
            ("cid-cat-home-indoor".to_string(), 88.0),
            ("cid-cat-outdoor".to_string(), 74.0),
            ("cid-dog-home".to_string(), 52.0),
            ("cid-home-interior".to_string(), 46.0),
        ]),
    };
    let task = DatasetSelectionTask {
        task_description:
            "Find labeled cat images in home indoor scenes for a classifier".into(),
        task_type: "classification".into(),
        required_columns: vec!["label".into()],
        target_entity: Some("cat".into()),
    };
    let config = DatasetValuationConfig {
        coarse_top_k: 4,
        metadata_weight: 0.6,
        utility_weight: 0.4,
        ..DatasetValuationConfig::default()
    };

    let output = engine
        .search_and_value(
            "cat home",
            &SearchFilters::default(),
            &local_metadata,
            &neutral_signal_fetcher(),
            None,
            Some(&sample_evaluator),
            Some(&task),
            &config,
            10,
        )
        .await
        .unwrap();

    dump_json("search.value.cat_home", &output);
    for candidate in &output.candidates {
        let plan = candidate.sample_plan.as_ref().unwrap();
        let proxy = candidate.proxy_utility.as_ref().unwrap();
        println!(
            "dataset_score cid={} title={} coarse={:.2} utility={:.2} final={:.2} sample_rows={} sample_bytes={}",
            candidate.result.cid.0,
            candidate.result.title,
            candidate.coarse_score,
            proxy.utility_score,
            candidate.final_score,
            plan.estimated_rows,
            plan.estimated_bytes,
        );
    }

    assert_eq!(output.candidates.len(), 4);
    assert!(output.errors.is_empty());
    assert_eq!(
        output.selected.as_ref().unwrap().result.cid.0,
        "cid-cat-home-indoor"
    );
    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| candidate.sample_plan.is_some())
    );
    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| candidate.proxy_utility.is_some())
    );
    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| (candidate.final_score - candidate.coarse_score).abs() > f64::EPSILON)
    );

    let indoor = output
        .candidates
        .iter()
        .find(|candidate| candidate.result.cid.0 == "cid-cat-home-indoor")
        .unwrap();
    let outdoor = output
        .candidates
        .iter()
        .find(|candidate| candidate.result.cid.0 == "cid-cat-outdoor")
        .unwrap();
    let home = output
        .candidates
        .iter()
        .find(|candidate| candidate.result.cid.0 == "cid-home-interior")
        .unwrap();
    let dog = output
        .candidates
        .iter()
        .find(|candidate| candidate.result.cid.0 == "cid-dog-home")
        .unwrap();

    assert!(indoor.final_score > outdoor.final_score);
    assert!(outdoor.final_score > dog.final_score);
    assert!(dog.final_score > home.final_score);
    assert_eq!(indoor.sample_plan.as_ref().unwrap().estimated_rows, 180);
    assert_eq!(outdoor.sample_plan.as_ref().unwrap().estimated_rows, 190);
    assert_eq!(dog.sample_plan.as_ref().unwrap().estimated_rows, 160);
    assert_eq!(home.sample_plan.as_ref().unwrap().estimated_rows, 170);
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

#[test]
fn default_adapters_covers_all_expected_sources() {
    let adapters = adapters::default_adapters();
    let names: Vec<&str> = adapters.iter().map(|a| a.name()).collect();
    let sources: Vec<DataSource> = adapters.iter().map(|a| a.source_type()).collect();

    assert!(names.contains(&"kaggle"), "missing kaggle adapter");
    assert!(names.contains(&"huggingface"), "missing huggingface adapter");
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

    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(names.len(), unique.len(), "duplicate adapter names detected");

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

#[tokio::test]
async fn unconfigured_adapters_return_empty_without_error() {
    let adapters = adapters::default_adapters();
    for adapter in &adapters {
        let result = adapter.search("test query", 5).await;
        match result {
            Ok(results) => {
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
            Err(_) => {}
        }
    }
}

#[test]
fn adapter_metadata_is_consistent() {
    let adapters = adapters::default_adapters();
    for adapter in &adapters {
        assert!(!adapter.name().is_empty(), "adapter name must not be empty");
        let source_json = serde_json::to_string(&adapter.source_type()).unwrap();
        assert!(
            source_json.len() > 2,
            "source_type serialization too short for {}",
            adapter.name()
        );
    }
}

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
        assert_eq!(json, expected_json, "serialization mismatch for {:?}", variant);
        let back: DataSource = serde_json::from_str(&json).unwrap();
        assert_eq!(
            serde_json::to_string(&back).unwrap(),
            json,
            "roundtrip failed for {json}"
        );
    }
}

#[test]
fn google_adapter_source_type_and_name() {
    let adapter = adapters::GoogleDatasetSearchAdapter::default();
    assert_eq!(adapter.name(), "google_dataset_search");
    assert!(matches!(
        adapter.source_type(),
        DataSource::GoogleDatasetSearch
    ));
}

#[test]
fn datacite_adapter_source_type_and_name() {
    let adapter = adapters::DataCiteCommonsAdapter::default();
    assert_eq!(adapter.name(), "datacite_commons");
    assert!(matches!(
        adapter.source_type(),
        DataSource::DataCiteCommons
    ));
}

#[tokio::test]
async fn search_engine_includes_new_adapter_results() {
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

    let filters_match = SearchFilters {
        source: Some("googledatasetsearch".into()),
        ..Default::default()
    };
    let output = engine
        .search(
            "test",
            &filters_match,
            &[],
            &neutral_signal_fetcher(),
            10,
        )
        .await
        .unwrap();
    assert_eq!(output.results.len(), 1);

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
    assert_eq!(infer_data_type_from_title("podcast_ep1.mp3"), DataType::Audio);
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
    assert_eq!(
        infer_data_type_from_title("random stuff here"),
        DataType::Tabular
    );
}

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
    assert!(results[0].schema.columns.len() >= 2);
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
    let results = adapter.search("ticker", 10).await.unwrap();
    assert_eq!(results.len(), 1);
}

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
    assert_eq!(output.results[0].result.data_type, DataType::Tabular);
}

#[tokio::test]
#[ignore]
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
    let result = adapter.search("zzzxxx_nonexistent_dataset_42", 3).await;
    assert!(result.is_ok(), "should not error on obscure query");
}

#[tokio::test]
#[ignore]
async fn datacite_commons_live_result_has_description_or_year() {
    let adapter = adapters::DataCiteCommonsAdapter::default();
    let results = adapter.search("genomics", 3).await.expect("API call failed");

    if !results.is_empty() {
        let has_desc = results.iter().any(|r| r.description.is_some());
        assert!(has_desc, "expected at least one result with a description");
    }
}
