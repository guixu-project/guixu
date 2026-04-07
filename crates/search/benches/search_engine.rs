// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use data_core::types::*;
use data_search::adapters::ExternalAdapter;
use data_search::engine::{SearchEngine, SearchFilters};
use data_search::intent::{IntentParser, QueryProfile};
use data_search::vector_index::VectorIndex;

// ---------------------------------------------------------------------------
// Stub adapter that returns N results instantly (no I/O)
// ---------------------------------------------------------------------------

struct BenchAdapter {
    id: &'static str,
    results: Vec<SearchResult>,
}

impl BenchAdapter {
    fn with_n(id: &'static str, n: usize) -> Self {
        let results = (0..n)
            .map(|i| SearchResult {
                cid: DatasetCid(format!("{id}-{i}")),
                title: format!("Dataset {i} from {id}"),
                description: Some(format!("A benchmark dataset for testing search ranking")),
                tags: vec!["benchmark".into(), id.into()],
                schema: DatasetSchema {
                    columns: vec![
                        ColumnDef {
                            name: "col_a".into(),
                            dtype: "int".into(),
                            nullable: true,
                            description: None,
                        },
                        ColumnDef {
                            name: "col_b".into(),
                            dtype: "text".into(),
                            nullable: true,
                            description: None,
                        },
                    ],
                    row_count: 1000 * (i as u64 + 1),
                    size_bytes: 50_000 * (i as u64 + 1),
                },
                quality: None,
                price: Price::free(),
                license: License {
                    spdx_id: "MIT".into(),
                    commercial_use: true,
                    derivative_allowed: true,
                },
                provider: Did(format!("did:{id}:{i}")),
                source: DataSource::HuggingFace,
                market: Some(DatasetMarketStats {
                    download_count: i as u64 * 100,
                    review_count: i as u64,
                    trade_count: 0,
                }),
                data_type: DataType::Tabular,
                created_at: chrono::Utc::now(),
                seller_endpoint: None,
                source_attributes: Some(serde_json::json!({ "skill_id": id })),
                provider_meta: Some(ProviderMeta {
                    provider_id: id.to_string(),
                    source_family: SourceFamily::Marketplace,
                    labels: vec![],
                }),
                governance: None,
            })
            .collect();
        Self { id, results }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for BenchAdapter {
    fn name(&self) -> &str {
        self.id
    }
    fn skill_id(&self) -> &str {
        self.id
    }
    fn source_family(&self) -> SourceFamily {
        SourceFamily::Marketplace
    }
    async fn search(&self, _query: &str, limit: usize) -> anyhow::Result<Vec<SearchResult>> {
        Ok(self.results.iter().take(limit).cloned().collect())
    }
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_search_with_profile(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // 5 adapters × 20 results each = 100 candidates to rank
    let adapters: Vec<Box<dyn ExternalAdapter>> = vec![
        Box::new(BenchAdapter::with_n("adapter_a", 20)),
        Box::new(BenchAdapter::with_n("adapter_b", 20)),
        Box::new(BenchAdapter::with_n("adapter_c", 20)),
        Box::new(BenchAdapter::with_n("adapter_d", 20)),
        Box::new(BenchAdapter::with_n("adapter_e", 20)),
    ];
    let engine = SearchEngine::new(VectorIndex, IntentParser, adapters);

    let profile = QueryProfile {
        raw_query: "image classification dataset for safety helmet detection".into(),
        task_type: Some("classification".into()),
        task_description: Some("image classification dataset for safety helmet detection".into()),
        target_entity: None,
        keywords: vec![
            "image".into(),
            "classification".into(),
            "safety".into(),
            "helmet".into(),
        ],
        data_standard: Default::default(),
        user_profile: Default::default(),
    };
    let filters = SearchFilters::default();
    let signal_fetcher: Box<dyn Fn(&str) -> data_core::feedback::CommunitySignal + Send + Sync> =
        Box::new(|cid: &str| data_core::feedback::CommunitySignal {
            dataset_cid: DatasetCid(cid.to_string()),
            total_reviews: 0,
            avg_relevance: 0.0,
            avg_quality: 0.0,
            positive_rate: 0.0,
            negative_rate: 0.0,
            task_signals: vec![],
        });

    c.bench_function("search_with_profile_5x20", |b| {
        b.iter(|| {
            rt.block_on(async {
                let output = engine
                    .search_with_profile(
                        black_box(&profile),
                        black_box(&filters),
                        black_box(&[]),
                        black_box(&signal_fetcher),
                        black_box(10),
                    )
                    .await
                    .unwrap();
                assert_eq!(output.results.len(), 10);
            })
        })
    });
}

fn bench_search_single_adapter(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let adapters: Vec<Box<dyn ExternalAdapter>> =
        vec![Box::new(BenchAdapter::with_n("single", 50))];
    let engine = SearchEngine::new(VectorIndex, IntentParser, adapters);

    let profile = QueryProfile {
        raw_query: "tabular financial data".into(),
        task_type: Some("forecasting".into()),
        task_description: Some("tabular financial data".into()),
        target_entity: None,
        keywords: vec!["tabular".into(), "financial".into()],
        data_standard: Default::default(),
        user_profile: Default::default(),
    };
    let filters = SearchFilters::default();
    let signal_fetcher: Box<dyn Fn(&str) -> data_core::feedback::CommunitySignal + Send + Sync> =
        Box::new(|cid: &str| data_core::feedback::CommunitySignal {
            dataset_cid: DatasetCid(cid.to_string()),
            total_reviews: 5,
            avg_relevance: 0.8,
            avg_quality: 4.0,
            positive_rate: 0.9,
            negative_rate: 0.05,
            task_signals: vec![],
        });

    c.bench_function("search_with_profile_1x50", |b| {
        b.iter(|| {
            rt.block_on(async {
                let output = engine
                    .search_with_profile(
                        black_box(&profile),
                        black_box(&filters),
                        black_box(&[]),
                        black_box(&signal_fetcher),
                        black_box(20),
                    )
                    .await
                    .unwrap();
                assert!(!output.results.is_empty());
            })
        })
    });
}

criterion_group!(
    benches,
    bench_search_with_profile,
    bench_search_single_adapter
);
criterion_main!(benches);
