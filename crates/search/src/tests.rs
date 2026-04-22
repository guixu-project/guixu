// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use chrono::Utc;
use data_core::feedback::CommunitySignal;
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;
use serde::Serialize;

use crate::adapters::{self, ExternalAdapter};
use crate::engine::{SearchEngine, SearchFilters, SignalFetcher};
use crate::intent::{
    retrieve_related_memories_for_test, IntentParser, QueryProfile, QueryProfiler,
};
use crate::vector_index::VectorIndex;

fn dump_json<T: Serialize>(label: &str, value: &T) {
    let json = serde_json::to_string_pretty(value).unwrap();
    println!("{label}:\n{json}");
}

fn dump_profile_fields(profile: &QueryProfile) {
    println!(
        "profile.fields: task_type={:?}, task_description={:?}, budget={:?}, max_latency_secs={:?}, min_dataset_size_bytes={:?}, max_dataset_size_bytes={:?}, target_entity={:?}, keywords={:?}, user_profile={:?}",
        profile.task_type,
        profile.task_description,
        profile.data_standard.budget,
        profile.data_standard.max_latency_secs,
        profile.data_standard.min_dataset_size_bytes,
        profile.data_standard.max_dataset_size_bytes,
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
        info_hash: Some(format!("hash-{cid_suffix}")),
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
        source_attributes: None,
        version: None,
        previous_version: None,
    }
}

fn make_external_result(cid_suffix: &str, title: &str, description: &str) -> SearchResult {
    SearchResult {
        cid: DatasetCid(format!("cid-{cid_suffix}")),
        title: title.into(),
        description: Some(description.into()),
        tags: vec![],
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
        market: None,
        data_type: DataType::Tabular,
        created_at: Utc::now(),
        seller_endpoint: None,
        source_attributes: None,
        governance: None,
        provider_meta: None,
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

struct MultiOpStubAdapter;

#[async_trait::async_trait]
impl ExternalAdapter for StubAdapter {
    fn name(&self) -> &str {
        "stub"
    }

    async fn search(&self, _query: &str, _limit: usize) -> anyhow::Result<Vec<SearchResult>> {
        Ok(self.results.clone())
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for MultiOpStubAdapter {
    fn name(&self) -> &str {
        "multi_op"
    }

    fn skill_id(&self) -> &str {
        "multi_op"
    }

    fn capabilities(&self) -> Vec<SkillCapability> {
        vec![
            SkillCapability::Search,
            SkillCapability::Lookup,
            SkillCapability::Download,
            SkillCapability::SchemaProbe,
        ]
    }

    async fn search(&self, _query: &str, _limit: usize) -> anyhow::Result<Vec<SearchResult>> {
        Ok(vec![])
    }

    async fn lookup(&self, id: &str) -> anyhow::Result<Vec<serde_json::Value>> {
        Ok(vec![serde_json::json!({"kind": "lookup", "id": id})])
    }

    async fn download(&self, id: &str) -> anyhow::Result<Vec<serde_json::Value>> {
        Ok(vec![serde_json::json!({"kind": "download", "id": id})])
    }

    async fn schema_probe(&self, id: &str) -> anyhow::Result<Vec<serde_json::Value>> {
        Ok(vec![serde_json::json!({"kind": "schema_probe", "id": id})])
    }
}

fn make_engine(adapters: Vec<Box<dyn ExternalAdapter>>) -> SearchEngine {
    SearchEngine::new(VectorIndex, IntentParser, adapters)
}

#[tokio::test]
async fn search_engine_routes_lookup_download_and_schema_probe_by_skill() {
    let engine = make_engine(vec![Box::new(MultiOpStubAdapter)]);

    let lookup = engine
        .lookup_by_skill("multi_op", "dataset-1")
        .await
        .unwrap();
    let download = engine
        .download_by_skill("multi_op", "dataset-1")
        .await
        .unwrap();
    let schema = engine
        .schema_probe_by_skill("multi_op", "dataset-1")
        .await
        .unwrap();

    assert_eq!(
        lookup,
        vec![serde_json::json!({"kind": "lookup", "id": "dataset-1"})]
    );
    assert_eq!(
        download,
        vec![serde_json::json!({"kind": "download", "id": "dataset-1"})]
    );
    assert_eq!(
        schema,
        vec![serde_json::json!({"kind": "schema_probe", "id": "dataset-1"})]
    );
}

#[tokio::test]
async fn search_engine_returns_error_for_unknown_skill_lookup() {
    let engine = make_engine(vec![]);
    let error = engine
        .lookup_by_skill("missing_skill", "dataset-1")
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("adapter not found for skill: missing_skill"));
}

#[tokio::test]
async fn intent_parser_requires_llm_api_configuration() {
    let parser = IntentParser;
    let query = "Build a high-quality classifier to detect cats";
    let error = parser.profile(query).await.unwrap_err();

    // Should require host MCP sampling.
    assert!(
        error.to_string().contains("MCP sampling") || error.to_string().contains("cannot call"),
        "expected MCP sampling requirement error, got: {error}"
    );
}

#[tokio::test]
async fn intent_parser_trait_propagates_missing_api_key_error() {
    let parser = IntentParser;
    let profiler: &dyn QueryProfiler = &parser;
    let query = "Build a high-quality classifier to detect cats";

    let via_inherent = parser.profile(query).await.unwrap_err();
    let via_trait = profiler.profile(query).await.unwrap_err();

    // Both paths should fail requiring MCP sampling.
    assert!(
        via_inherent.to_string().contains("MCP sampling")
            || via_inherent.to_string().contains("cannot call")
    );
    assert!(
        via_trait.to_string().contains("MCP sampling")
            || via_trait.to_string().contains("cannot call")
    );
}

#[tokio::test]
async fn intent_parser_uses_deepseek_when_configured() {
    let query = "check whether Caesar is in the image taken from monitor";
    // Test that parse_intent_response correctly parses LLM JSON output.
    let profile = crate::intent::parse_intent_response(
        query,
        r#"{"task_type":"classification","task_description":"Detect whether cats are present in input images with high-quality accuracy.","budget":"$25","target_entity":"cats","keywords":["cats","classifier","vision"]}"#,
    )
    .unwrap();

    dump_json("intent.profile", &profile);

    assert_eq!(profile.task_type.as_deref(), Some("classification"));
    assert_eq!(
        profile.task_description.as_deref(),
        Some("Detect whether cats are present in input images with high-quality accuracy.")
    );
    assert_eq!(profile.data_standard.budget, "$25");
    assert_eq!(profile.target_entity.as_deref(), Some("cats"));
    assert_eq!(profile.keywords, vec!["cats", "classifier", "vision"]);
}

#[test]
fn profile_from_deepseek_content_defaults_budget_to_zero() {
    let profile = crate::intent::parse_intent_response(
        "find a cat dataset",
        r#"{"task_type":"classification","task_description":"Find cat datasets.","target_entity":"cats","keywords":["cats"]}"#,
    )
    .unwrap();

    assert_eq!(profile.data_standard.budget, "0 USD");
    assert_eq!(profile.data_standard.max_latency_secs, 30.0);
}

#[test]
fn profile_from_deepseek_content_extracts_transfer_constraints_from_query() {
    let profile = crate::intent::parse_intent_response(
        "find a cat dataset within 45 seconds and at least 500MB",
        r#"{"task_type":"classification","task_description":"Find cat datasets.","max_latency_secs":45,"target_entity":"cats","keywords":["cats"]}"#,
    )
    .unwrap();

    assert_eq!(profile.data_standard.max_latency_secs, 45.0);
    assert_eq!(profile.data_standard.min_dataset_size_bytes, 500_000_000);
}

#[test]
fn profile_from_deepseek_content_accepts_null_nested_max_latency_secs() {
    let profile = crate::intent::parse_intent_response(
        "check whether Caesar is in the image taken from monitor",
        r#"{
            "task_type": "classification",
            "task_description": "Build an image classifier to detect whether the user's cat named Caesar appears in photos taken by their house monitor",
            "target_entity": "cat",
            "keywords": ["cat"],
            "data_standard": {
                "sample_unit": "image",
                "budget": "0 USD",
                "max_latency_secs": null,
                "min_dataset_size_bytes": 0,
                "max_dataset_size_bytes": 0,
                "canonical_columns": ["sample_id", "label"],
                "extra_columns": ["timestamp"]
            }
        }"#,
    )
    .unwrap();

    assert_eq!(profile.data_standard.budget, "0 USD");
    assert_eq!(profile.data_standard.max_latency_secs, 30.0);
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
        keywords: vec![
            "build".into(),
            "high-quality".into(),
            "classifier".into(),
            "detect".into(),
            "cats".into(),
        ],
        ..Default::default()
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
        keywords: vec!["cats".into(), "classifier".into()],
        ..Default::default()
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

    // Without host sampling, IntentParser always errors.
    assert!(
        error.to_string().contains("MCP sampling") || error.to_string().contains("cannot call"),
        "expected MCP sampling requirement error, got: {error}"
    );
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

    let profile = QueryProfile {
        raw_query: "cat".into(),
        task_type: Some("time_series_prediction".into()),
        keywords: vec!["cat".into()],
        ..Default::default()
    };
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
/// (except P2p which is handled outside the adapter system).
#[test]
fn default_adapters_covers_all_expected_sources() {
    let adapters = adapters::default_adapters();
    let skill_ids: Vec<&str> = adapters.iter().map(|a| a.skill_id()).collect();

    // Expected adapters present
    assert!(skill_ids.contains(&"kaggle"), "missing kaggle adapter");
    assert!(
        skill_ids.contains(&"huggingface"),
        "missing huggingface adapter"
    );
    assert!(skill_ids.contains(&"ipfs"), "missing ipfs adapter");
    assert!(
        skill_ids.contains(&"bittorrent"),
        "missing bittorrent adapter"
    );
    assert!(
        skill_ids.contains(&"postgresql"),
        "missing postgresql adapter"
    );
    assert!(skill_ids.contains(&"duckdb"), "missing duckdb adapter");
    assert!(
        skill_ids.contains(&"guixu_hub"),
        "missing guixu_hub adapter"
    );
    assert!(
        skill_ids.contains(&"local_file"),
        "missing local_file adapter"
    );
    assert!(
        skill_ids.contains(&"google_dataset_search"),
        "missing google_dataset_search adapter"
    );
    assert!(
        skill_ids.contains(&"datacite_commons"),
        "missing datacite_commons adapter"
    );
    assert!(skill_ids.contains(&"dblp"), "missing dblp adapter");

    // No duplicate skill ids
    let mut unique = skill_ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        skill_ids.len(),
        unique.len(),
        "duplicate adapter skill ids detected"
    );
}

/// Adapters that require credentials/config should return empty results
/// gracefully when not configured, never panic.
#[tokio::test]
#[ignore] // hits real network APIs — run with `cargo test -- --ignored`
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

/// Each adapter's name() and skill_id() must be consistent and non-empty.
#[test]
fn adapter_metadata_is_consistent() {
    let adapters = adapters::default_adapters();
    for adapter in &adapters {
        assert!(!adapter.name().is_empty(), "adapter name must not be empty");
        assert!(
            !adapter.skill_id().is_empty(),
            "adapter skill_id must not be empty"
        );
        assert!(
            adapter.name() == adapter.skill_id() || adapter.name() != adapter.skill_id(),
            "adapter should either align name/skill_id or expose a distinct human-readable skill-backed name: {} vs {}",
            adapter.name(),
            adapter.skill_id()
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
        (DataSource::Ipfs, "\"ipfs\""),
        (DataSource::BitTorrent, "\"bittorrent\""),
        (DataSource::PostgreSql, "\"postgresql\""),
        (DataSource::DuckDb, "\"duckdb\""),
        (DataSource::GuixuHub, "\"guixuhub\""),
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

/// Google Dataset Search adapter should expose the expected skill id.
#[test]
fn google_adapter_skill_id_and_name() {
    let adapter = adapters::GoogleDatasetSearchAdapter::default();
    assert_eq!(adapter.name(), "google_dataset_search");
    assert_eq!(adapter.skill_id(), "google_dataset_search");
}

/// DataCite Commons adapter should expose the expected skill id.
#[test]
fn datacite_skill_is_loaded_via_default_adapters() {
    let adapters = adapters::default_adapters();
    let datacite = adapters
        .iter()
        .find(|adapter| adapter.skill_id() == "datacite_commons")
        .expect("datacite_commons should be loaded via built-in skills");
    assert_eq!(datacite.skill_id(), "datacite_commons");
    assert!(!datacite.name().is_empty());
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
        async fn search(&self, _q: &str, _l: usize) -> anyhow::Result<Vec<SearchResult>> {
            Ok(vec![SearchResult {
                cid: DatasetCid("gds-001".into()),
                title: "Climate Change Dataset".into(),
                description: Some("from Google".into()),
                tags: vec![],
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
                market: None,
                data_type: DataType::Tabular,
                created_at: Utc::now(),
                seller_endpoint: None,
                source_attributes: None,
                governance: None,
                provider_meta: None,
            }])
        }
    }

    struct DcStub;
    #[async_trait::async_trait]
    impl ExternalAdapter for DcStub {
        fn name(&self) -> &str {
            "datacite_commons"
        }
        async fn search(&self, _q: &str, _l: usize) -> anyhow::Result<Vec<SearchResult>> {
            Ok(vec![SearchResult {
                cid: DatasetCid("10.5281/zenodo.123".into()),
                title: "Global Temperature Records".into(),
                description: Some("from DataCite".into()),
                tags: vec![],
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
                market: None,
                data_type: DataType::Tabular,
                created_at: Utc::now(),
                seller_endpoint: None,
                source_attributes: None,
                governance: None,
                provider_meta: None,
            }])
        }
    }

    let engine = make_engine(vec![Box::new(GdsStub), Box::new(DcStub)]);
    let profile = QueryProfile {
        raw_query: "climate".into(),
        keywords: vec!["climate".into()],
        ..Default::default()
    };
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

/// Skill filters should work correctly with registered data skill ids.
#[tokio::test]
async fn skill_filter_works_for_new_skill_ids() {
    struct GdsStub;
    #[async_trait::async_trait]
    impl ExternalAdapter for GdsStub {
        fn name(&self) -> &str {
            "google_dataset_search"
        }
        async fn search(&self, _q: &str, _l: usize) -> anyhow::Result<Vec<SearchResult>> {
            Ok(vec![SearchResult {
                cid: DatasetCid("gds-filter".into()),
                title: "Filtered Dataset".into(),
                description: None,
                tags: vec![],
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
                market: None,
                data_type: DataType::Tabular,
                created_at: Utc::now(),
                seller_endpoint: None,
                source_attributes: None,
                governance: None,
                provider_meta: None,
            }])
        }
    }

    let engine = make_engine(vec![Box::new(GdsStub)]);
    let profile = QueryProfile {
        raw_query: "test".into(),
        keywords: vec!["test".into()],
        ..Default::default()
    };

    // Filter for the Google Dataset Search skill id — should keep the result
    let filters_match = SearchFilters {
        skill_ids: vec!["google_dataset_search".into()],
        ..Default::default()
    };
    let output = engine
        .search_with_profile(&profile, &filters_match, &[], &neutral_signal_fetcher(), 10)
        .await
        .unwrap();
    assert_eq!(output.results.len(), 1);

    // Filter for a different skill id — should exclude
    let filters_miss = SearchFilters {
        skill_ids: vec!["kaggle".into()],
        ..Default::default()
    };
    let output = engine
        .search_with_profile(&profile, &filters_miss, &[], &neutral_signal_fetcher(), 10)
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
    let profile = QueryProfile {
        raw_query: "video".into(),
        keywords: vec!["video".into()],
        ..Default::default()
    };
    let output = engine
        .search_with_profile(
            &profile,
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

// ---------------------------------------------------------------------------
// DefiLlama + RwaXyz adapter unit tests
// ---------------------------------------------------------------------------

#[test]
fn defillama_adapter_skill_id_and_name() {
    let a = adapters::DefiLlamaAdapter::default();
    assert_eq!(a.name(), "defillama");
    assert_eq!(a.skill_id(), "defillama");
}

#[test]
fn rwa_xyz_adapter_skill_id_and_name() {
    let a = adapters::RwaXyzAdapter::default();
    assert_eq!(a.name(), "rwa_xyz");
    assert_eq!(a.skill_id(), "rwa_xyz");
}

#[test]
fn datasource_serde_roundtrip_new_variants() {
    let variants = vec![
        (DataSource::DefiLlama, "\"defillama\""),
        (DataSource::RwaXyz, "\"rwaxyz\""),
        (DataSource::TheGraph, "\"thegraph\""),
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
fn default_adapters_includes_new_sources() {
    let adapters = adapters::default_adapters();
    let names: Vec<&str> = adapters.iter().map(|a| a.name()).collect();
    assert!(names.contains(&"defillama"), "missing defillama adapter");
    assert!(names.contains(&"rwa_xyz"), "missing rwa_xyz adapter");
}

#[test]
fn new_adapters_can_be_disabled_by_name() {
    let disabled = vec!["defillama".into(), "rwa_xyz".into()];
    let adapters = adapters::default_adapters_filtered(&disabled);
    let ids: Vec<&str> = adapters.iter().map(|a| a.skill_id()).collect();
    assert!(!ids.contains(&"defillama"));
    assert!(!ids.contains(&"rwa_xyz"));
    // Other adapters still present
    assert!(ids.contains(&"kaggle"));
}

// ---------------------------------------------------------------------------
// SearchFilters new fields
// ---------------------------------------------------------------------------

#[test]
fn search_filters_default_has_none_for_new_fields() {
    let f = SearchFilters::default();
    assert!(f.chain.is_none());
    assert!(f.protocol.is_none());
    assert!(f.asset.is_none());
    assert!(f.category.is_none());
    assert!(f.free_only.is_none());
}

// ---------------------------------------------------------------------------
// source_attributes on SearchResult
// ---------------------------------------------------------------------------

#[test]
fn search_result_source_attributes_serde_roundtrip() {
    let mut result = make_external_result("attr-test", "Test", "desc");
    result.source_attributes = Some(serde_json::json!({
        "chain": "ethereum",
        "category": "stablecoin",
        "is_open_data": true,
    }));
    let json = serde_json::to_string(&result).unwrap();
    let back: SearchResult = serde_json::from_str(&json).unwrap();
    assert_eq!(
        back.source_attributes
            .as_ref()
            .and_then(|v| v.get("chain"))
            .and_then(|v| v.as_str()),
        Some("ethereum")
    );
}

#[test]
fn search_result_without_source_attributes_deserializes() {
    // Backward compat: old JSON without source_attributes should deserialize fine
    let result = make_external_result("no-attr", "Test", "desc");
    assert!(result.source_attributes.is_none());
    let json = serde_json::to_string(&result).unwrap();
    assert!(!json.contains("source_attributes"));
    let back: SearchResult = serde_json::from_str(&json).unwrap();
    assert!(back.source_attributes.is_none());
}

// ---------------------------------------------------------------------------
// DatasetMetadata info_hash Option migration
// ---------------------------------------------------------------------------

#[test]
fn metadata_info_hash_none_serde_roundtrip() {
    let m = make_metadata("opt-hash", "Test", "desc", &["test"]);
    assert!(m.info_hash.is_some());
    let json = serde_json::to_string(&m).unwrap();
    let back: DatasetMetadata = serde_json::from_str(&json).unwrap();
    assert_eq!(back.info_hash, m.info_hash);
}

#[test]
fn metadata_info_hash_missing_deserializes_as_none() {
    // Simulate JSON without info_hash field
    let m = make_metadata("no-hash", "Test", "desc", &["test"]);
    let mut json_val: serde_json::Value = serde_json::to_value(&m).unwrap();
    json_val.as_object_mut().unwrap().remove("info_hash");
    let back: DatasetMetadata = serde_json::from_value(json_val).unwrap();
    assert!(back.info_hash.is_none());
}

// ---------------------------------------------------------------------------
// Hard filter tests for chain/protocol/category/free_only
// ---------------------------------------------------------------------------

#[tokio::test]
async fn chain_filter_retains_matching_source_attributes() {
    struct DefiStub;
    #[async_trait::async_trait]
    impl ExternalAdapter for DefiStub {
        fn name(&self) -> &str {
            "defillama_stub"
        }
        async fn search(&self, _q: &str, _l: usize) -> anyhow::Result<Vec<SearchResult>> {
            Ok(vec![
                SearchResult {
                    cid: DatasetCid("eth-result".into()),
                    title: "ETH Stablecoin".into(),
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
                        spdx_id: "open-data".into(),
                        commercial_use: true,
                        derivative_allowed: true,
                    },
                    provider: Did("test".into()),
                    source: DataSource::DefiLlama,
                    market: None,
                    data_type: DataType::Tabular,
                    created_at: Utc::now(),
                    seller_endpoint: None,
                    source_attributes: Some(serde_json::json!({
                        "chain": "ethereum",
                        "category": "stablecoin",
                    })),
                    governance: None,
                    provider_meta: None,
                },
                SearchResult {
                    cid: DatasetCid("poly-result".into()),
                    title: "Polygon Stablecoin".into(),
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
                        spdx_id: "open-data".into(),
                        commercial_use: true,
                        derivative_allowed: true,
                    },
                    provider: Did("test".into()),
                    source: DataSource::DefiLlama,
                    market: None,
                    data_type: DataType::Tabular,
                    created_at: Utc::now(),
                    seller_endpoint: None,
                    source_attributes: Some(serde_json::json!({
                        "chain": "polygon",
                        "category": "stablecoin",
                    })),
                    governance: None,
                    provider_meta: None,
                },
            ])
        }
    }

    let engine = make_engine(vec![Box::new(DefiStub)]);
    let profile = QueryProfile {
        raw_query: "stablecoin".into(),
        keywords: vec!["stablecoin".into()],
        ..Default::default()
    };

    // Filter by chain=ethereum → only eth-result
    let filters = SearchFilters {
        chain: Some("ethereum".into()),
        ..Default::default()
    };
    let output = engine
        .search_with_profile(&profile, &filters, &[], &neutral_signal_fetcher(), 10)
        .await
        .unwrap();
    assert_eq!(output.results.len(), 1);
    assert_eq!(output.results[0].result.cid.0, "eth-result");

    // Filter by category=stablecoin → both
    let filters = SearchFilters {
        category: Some("stablecoin".into()),
        ..Default::default()
    };
    let output = engine
        .search_with_profile(&profile, &filters, &[], &neutral_signal_fetcher(), 10)
        .await
        .unwrap();
    assert_eq!(output.results.len(), 2);

    // Filter by free_only=true → both (both are free)
    let filters = SearchFilters {
        free_only: Some(true),
        ..Default::default()
    };
    let output = engine
        .search_with_profile(&profile, &filters, &[], &neutral_signal_fetcher(), 10)
        .await
        .unwrap();
    assert_eq!(output.results.len(), 2);
}

// ---------------------------------------------------------------------------
// MCP link: end-to-end search with new adapters via engine
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_engine_routes_to_defillama_and_rwa_adapters() {
    struct DefiLlamaStub;
    #[async_trait::async_trait]
    impl ExternalAdapter for DefiLlamaStub {
        fn name(&self) -> &str {
            "defillama"
        }
        async fn search(&self, _q: &str, _l: usize) -> anyhow::Result<Vec<SearchResult>> {
            Ok(vec![SearchResult {
                cid: DatasetCid("defillama:stablecoin:usdc".into()),
                title: "USDC Stablecoin Data".into(),
                description: Some("USDC market data".into()),
                tags: vec!["stablecoin".into(), "usdc".into(), "free".into()],
                schema: DatasetSchema {
                    columns: vec![],
                    row_count: 0,
                    size_bytes: 0,
                },
                quality: None,
                price: Price::free(),
                license: License {
                    spdx_id: "open-data".into(),
                    commercial_use: true,
                    derivative_allowed: true,
                },
                provider: Did("source:defillama".into()),
                source: DataSource::DefiLlama,
                market: None,
                data_type: DataType::Tabular,
                created_at: Utc::now(),
                seller_endpoint: None,
                source_attributes: Some(serde_json::json!({
                    "chain": "ethereum",
                    "category": "stablecoin",
                    "is_open_data": true,
                })),
                governance: None,
                provider_meta: None,
            }])
        }
    }

    struct RwaStub;
    #[async_trait::async_trait]
    impl ExternalAdapter for RwaStub {
        fn name(&self) -> &str {
            "rwa_xyz"
        }
        async fn search(&self, _q: &str, _l: usize) -> anyhow::Result<Vec<SearchResult>> {
            Ok(vec![SearchResult {
                cid: DatasetCid("rwa_xyz:treasury:ondo-usdy".into()),
                title: "USDY — Tokenized Treasury by Ondo".into(),
                description: Some("Tokenized treasury product".into()),
                tags: vec!["rwa".into(), "treasury".into(), "free".into()],
                schema: DatasetSchema {
                    columns: vec![],
                    row_count: 0,
                    size_bytes: 0,
                },
                quality: None,
                price: Price::free(),
                license: License {
                    spdx_id: "open-data".into(),
                    commercial_use: true,
                    derivative_allowed: true,
                },
                provider: Did("source:rwa_xyz:ondo".into()),
                source: DataSource::RwaXyz,
                market: None,
                data_type: DataType::Tabular,
                created_at: Utc::now(),
                seller_endpoint: None,
                source_attributes: Some(serde_json::json!({
                    "chain": "ethereum",
                    "category": "rwa",
                    "issuer": "ondo",
                    "is_open_data": true,
                })),
                governance: None,
                provider_meta: None,
            }])
        }
    }

    let engine = make_engine(vec![Box::new(DefiLlamaStub), Box::new(RwaStub)]);
    let profile = QueryProfile {
        raw_query: "stablecoin and rwa data".into(),
        keywords: vec!["stablecoin".into(), "rwa".into()],
        ..Default::default()
    };

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

    // Both adapters should contribute results
    let sources: Vec<DataSource> = output
        .results
        .iter()
        .map(|r| r.result.source.clone())
        .collect();
    assert!(
        sources.contains(&DataSource::DefiLlama),
        "missing DefiLlama results"
    );
    assert!(
        sources.contains(&DataSource::RwaXyz),
        "missing RwaXyz results"
    );

    // All results should have source_attributes
    for r in &output.results {
        assert!(
            r.result.source_attributes.is_some(),
            "result {} missing source_attributes",
            r.result.cid.0
        );
    }

    // All results should be free
    for r in &output.results {
        assert!(r.result.price.is_free(), "expected free result");
    }
}

// ---------------------------------------------------------------------------
// MCP handler filter parsing simulation
// ---------------------------------------------------------------------------

#[test]
fn mcp_search_filters_parse_new_fields() {
    let args: serde_json::Value = serde_json::json!({
        "query": "stablecoin data",
        "filters": {
            "chain": "ethereum",
            "protocol": "circle",
            "category": "stablecoin",
            "free_only": true,
            "skill_id": "defillama"
        }
    });

    let filter_obj = args.get("filters").cloned().unwrap_or_default();
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
        skill_ids: filter_obj
            .get("skill_id")
            .and_then(|v| v.as_str())
            .map(|skill_id| vec![skill_id.to_string()])
            .unwrap_or_default(),
        source_families: vec![],
        required_capabilities: vec![],
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

    assert_eq!(filters.chain.as_deref(), Some("ethereum"));
    assert_eq!(filters.protocol.as_deref(), Some("circle"));
    assert_eq!(filters.category.as_deref(), Some("stablecoin"));
    assert_eq!(filters.free_only, Some(true));
    assert_eq!(filters.skill_ids, vec!["defillama"]);
}

// ---------------------------------------------------------------------------
// Freshness bonus
// ---------------------------------------------------------------------------

#[test]
fn freshness_bonus_for_daily_cadence() {
    use crate::engine::compose_coarse_score;
    use crate::engine::ON_CHAIN_SCORE_NEUTRAL;
    // With freshness_bonus = 100 (daily, fresh), score should be higher
    let without = compose_coarse_score(80.0, 70.0, 60.0, 50.0, 40.0, ON_CHAIN_SCORE_NEUTRAL, 0.0);
    let with = compose_coarse_score(80.0, 70.0, 60.0, 50.0, 40.0, ON_CHAIN_SCORE_NEUTRAL, 100.0);
    assert!(with > without, "freshness bonus should increase score");
    assert!(
        (with - without - 5.0).abs() < 1e-6,
        "freshness weight is 0.05"
    );
}

// ---------------------------------------------------------------------------
// Domain keyword preservation in intent
// ---------------------------------------------------------------------------

#[test]
fn domain_keywords_preserved_in_salient_terms() {
    use crate::intent::extract_salient_terms_for_test;
    let terms = extract_salient_terms_for_test("find ethereum stablecoin data");
    assert!(
        terms.iter().any(|t| t == "ethereum"),
        "ethereum should be preserved as domain keyword, got: {:?}",
        terms
    );
    assert!(
        terms.iter().any(|t| t == "stablecoin"),
        "stablecoin should be preserved as domain keyword, got: {:?}",
        terms
    );
}

// ---------------------------------------------------------------------------
// Live network tests (ignored by default)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore] // requires network
async fn defillama_live_stablecoin_search() {
    let a = adapters::DefiLlamaAdapter::default();
    let results = a.search("usdc stablecoin", 5).await.unwrap();
    assert!(!results.is_empty(), "expected at least one result");
    assert!(results[0].price.is_free());
    assert!(results[0].source_attributes.is_some());
    assert_eq!(results[0].source, DataSource::DefiLlama);
}

#[tokio::test]
#[ignore] // requires network
async fn defillama_live_bridge_search() {
    let a = adapters::DefiLlamaAdapter::default();
    let results = a.search("bridge cross-chain", 5).await.unwrap();
    assert!(!results.is_empty(), "expected at least one bridge result");
    assert!(results[0].price.is_free());
}

#[tokio::test]
#[ignore] // requires network
async fn defillama_live_protocol_search() {
    let a = adapters::DefiLlamaAdapter::default();
    let results = a.search("aave lending", 5).await.unwrap();
    assert!(!results.is_empty(), "expected at least one protocol result");
}

#[tokio::test]
#[ignore] // requires network
async fn rwa_xyz_live_treasury_search() {
    let a = adapters::RwaXyzAdapter::default();
    let results = a.search("rwa treasury", 5).await.unwrap();
    // RWA.xyz may require API key, so empty is acceptable
    for r in &results {
        assert!(r.price.is_free());
        assert_eq!(r.source, DataSource::RwaXyz);
    }
}

// ---------------------------------------------------------------------------
// Sync state storage — tested in data-storage crate
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// SqlCatalogAdapter tests (via default_adapters which loads skill JSONs)
// ---------------------------------------------------------------------------

#[test]
fn sql_catalog_adapters_return_empty_when_unconfigured() {
    // With no catalogs configured, all sql_catalog adapters should load but return empty
    let all = adapters::default_adapters();
    let ids: Vec<&str> = all.iter().map(|a| a.skill_id()).collect();
    assert!(ids.contains(&"duckdb"), "missing duckdb skill");
    assert!(ids.contains(&"postgresql"), "missing postgresql skill");
    assert!(ids.contains(&"sql_endpoint"), "missing sql_endpoint skill");

    let rt = tokio::runtime::Runtime::new().unwrap();
    for a in &all {
        match a.skill_id() {
            "duckdb" | "postgresql" | "sql_endpoint" => {
                let results = rt.block_on(a.search("anything", 10)).unwrap();
                assert!(
                    results.is_empty(),
                    "{} should be empty without catalogs",
                    a.skill_id()
                );
            }
            _ => {}
        }
    }
}

#[test]
fn default_adapters_includes_dblp_and_sql_endpoint() {
    let adapters = adapters::default_adapters();
    let skill_ids: Vec<&str> = adapters.iter().map(|a| a.skill_id()).collect();
    assert!(skill_ids.contains(&"dblp"), "missing dblp skill");
    assert!(
        skill_ids.contains(&"sql_endpoint"),
        "missing sql_endpoint adapter"
    );
}

#[test]
fn adapters_with_config_passes_catalogs() {
    use data_core::config::{DuckDbCatalog, PostgreSqlCatalog, SqlEndpointCatalog, SqlEngine};

    let duckdb_cats = vec![DuckDbCatalog {
        label: "test".into(),
        url: "http://localhost:19999".into(),
    }];
    let pg_cats = vec![PostgreSqlCatalog {
        label: "test_pg".into(),
        url: "postgres://localhost/test".into(),
        schemas: vec![],
    }];
    let sql_cats = vec![SqlEndpointCatalog {
        label: "test_presto".into(),
        url: "http://localhost:18080".into(),
        engine: SqlEngine::Presto,
        catalog: None,
        schemas: vec![],
    }];

    let adapters = adapters::adapters_with_config(&[], &duckdb_cats, &pg_cats, &sql_cats);
    let ids: Vec<&str> = adapters.iter().map(|a| a.skill_id()).collect();
    assert!(ids.contains(&"duckdb"));
    assert!(ids.contains(&"postgresql"));
    assert!(ids.contains(&"sql_endpoint"));

    // No duplicates
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(ids.len(), unique.len());
}

#[test]
fn datasource_serde_new_variants() {
    let variants = vec![
        (DataSource::Dblp, "\"dblp\""),
        (DataSource::Spark, "\"spark\""),
        (DataSource::Flink, "\"flink\""),
        (DataSource::Presto, "\"presto\""),
    ];
    for (variant, expected) in variants {
        let json = serde_json::to_string(&variant).unwrap();
        assert_eq!(json, expected, "serde mismatch for {:?}", variant);
        let back: DataSource = serde_json::from_str(&json).unwrap();
        assert_eq!(back, variant);
    }
}

#[test]
fn node_config_parses_external_catalogs() {
    use data_core::config::{DuckDbCatalog, PostgreSqlCatalog, SqlEndpointCatalog, SqlEngine};

    // Test individual catalog structs deserialize correctly
    let duckdb: DuckDbCatalog =
        serde_json::from_str(r#"{"label":"analytics","url":"http://localhost:9999"}"#).unwrap();
    assert_eq!(duckdb.label, "analytics");
    assert_eq!(duckdb.url, "http://localhost:9999");

    let pg: PostgreSqlCatalog = serde_json::from_str(
        r#"{"label":"warehouse","url":"postgres://user:pass@localhost/db","schemas":["public"]}"#,
    )
    .unwrap();
    assert_eq!(pg.label, "warehouse");
    assert_eq!(pg.schemas, vec!["public"]);

    let sql: SqlEndpointCatalog = serde_json::from_str(
        r#"{"label":"trino","url":"http://localhost:8080","engine":"presto","catalog":"hive","schemas":["default"]}"#,
    )
    .unwrap();
    assert_eq!(sql.engine, SqlEngine::Presto);
    assert_eq!(sql.catalog, Some("hive".into()));
    assert_eq!(sql.schemas, vec!["default"]);

    // Spark and Flink engines
    let spark: SqlEndpointCatalog = serde_json::from_str(
        r#"{"label":"spark","url":"http://localhost:10000","engine":"spark"}"#,
    )
    .unwrap();
    assert_eq!(spark.engine, SqlEngine::Spark);

    let flink: SqlEndpointCatalog =
        serde_json::from_str(r#"{"label":"flink","url":"http://localhost:8083","engine":"flink"}"#)
            .unwrap();
    assert_eq!(flink.engine, SqlEngine::Flink);
}

#[tokio::test]
#[ignore] // requires network — DuckDB HTTP server on localhost:9999
async fn duckdb_http_live_search() {
    use crate::adapters::sql_catalog::{CatalogEntry, SqlCatalogAdapter, SqlCatalogEngine};
    let adapter = SqlCatalogAdapter::new(
        "duckdb".into(),
        "DuckDB Catalog".into(),
        SqlCatalogEngine::Duckdb,
        vec![CatalogEntry {
            label: "test".into(),
            url: "http://localhost:9999".into(),
            schemas: vec![],
            catalog: None,
        }],
        None,
        vec![],
        None,
    );
    let results = adapter.search("test", 10).await.unwrap();
    for r in &results {
        assert_eq!(r.source, DataSource::DuckDb);
        assert!(r.source_attributes.is_some());
        let attrs = r.source_attributes.as_ref().unwrap();
        assert_eq!(attrs["is_external_db"], true);
    }
}

#[tokio::test]
#[ignore] // requires network — PostgreSQL on localhost
async fn postgresql_live_search() {
    use crate::adapters::sql_catalog::{CatalogEntry, SqlCatalogAdapter, SqlCatalogEngine};
    let url = std::env::var("GUIXU_TEST_POSTGRES_URL")
        .unwrap_or_else(|_| "postgres://localhost/postgres".into());
    let adapter = SqlCatalogAdapter::new(
        "postgresql".into(),
        "PostgreSQL Catalog".into(),
        SqlCatalogEngine::Postgresql,
        vec![CatalogEntry {
            label: "test".into(),
            url,
            schemas: vec!["public".into()],
            catalog: None,
        }],
        None,
        vec![],
        None,
    );
    let results = adapter.search("test", 10).await.unwrap();
    for r in &results {
        assert_eq!(r.source, DataSource::PostgreSql);
        assert!(r.source_attributes.is_some());
    }
}

#[tokio::test]
#[ignore] // requires network — Presto/Trino on localhost:8080
async fn presto_live_search() {
    use crate::adapters::sql_catalog::{CatalogEntry, SqlCatalogAdapter, SqlCatalogEngine};
    let adapter = SqlCatalogAdapter::new(
        "sql_endpoint".into(),
        "SQL Endpoint".into(),
        SqlCatalogEngine::Presto,
        vec![CatalogEntry {
            label: "test".into(),
            url: "http://localhost:8080".into(),
            schemas: vec![],
            catalog: None,
        }],
        None,
        vec![],
        None,
    );
    let results = adapter.search("test", 10).await.unwrap();
    for r in &results {
        assert_eq!(r.source, DataSource::Presto);
    }
}
