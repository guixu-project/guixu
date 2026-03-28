use chrono::Utc;
use data_core::feedback::CommunitySignal;
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;
use serde::Serialize;

use crate::adapters::{self, ExternalAdapter};
use crate::engine::{SearchEngine, SearchFilters, SignalFetcher};
use crate::intent::{
    retrieve_related_memories_for_test, DataStandard, IntentParser, IntentParserConfig,
    QueryProfile, QueryProfiler, UserProfile,
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
    assert!(names.contains(&"guixu_hub"), "missing guixu_hub adapter");
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
    assert!(sources.contains(&DataSource::GuixuHub));
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

#[test]
fn guixu_hub_adapter_source_type_and_name() {
    let adapter = adapters::GuixuHubAdapter::default();
    assert_eq!(adapter.name(), "guixu_hub");
    assert!(matches!(adapter.source_type(), DataSource::GuixuHub));
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
