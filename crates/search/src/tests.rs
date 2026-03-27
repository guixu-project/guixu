use chrono::Utc;
use data_core::feedback::CommunitySignal;
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;
use serde::Serialize;

use crate::adapters::ExternalAdapter;
use crate::engine::{SearchEngine, SearchFilters, SignalFetcher};
use crate::intent::{IntentParser, QueryProfile, QueryProfiler};
use crate::vector_index::VectorIndex;

fn dump_json<T: Serialize>(label: &str, value: &T) {
    let json = serde_json::to_string_pretty(value).unwrap();
    println!("{label}:\n{json}");
}

fn dump_profile_fields(profile: &QueryProfile) {
    println!(
        "profile.fields: task_type={:?}, target_entity={:?}, quality_hint={:?}, keywords={:?}",
        profile.task_type,
        profile.target_entity,
        profile.quality_hint,
        profile.keywords
    );
}

fn make_metadata(cid_suffix: &str, title: &str, description: &str, tags: &[&str]) -> DatasetMetadata {
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
        data_type: DataType::Tabular,
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

    assert_eq!(profile.raw_query, "Build a high-quality classifier to detect cats");
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
    assert_eq!(output.results[0].result.title, "Cat Image Classification Dataset");
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

    let cids: Vec<&str> = output.results.iter().map(|r| r.result.cid.0.as_str()).collect();
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
    assert_eq!(via_wrapper.results[0].result.cid.0, via_profile.results[0].result.cid.0);
}
