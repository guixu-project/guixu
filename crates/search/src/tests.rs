use chrono::Utc;
use data_core::feedback::CommunitySignal;
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;
use serde::Serialize;

use crate::adapters::ExternalAdapter;
use crate::engine::{SearchEngine, SearchFilters, SignalFetcher};
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
