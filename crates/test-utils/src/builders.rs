// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Builder pattern for complex domain objects used in tests.
//!
//! When `DatasetMetadata` or other structs gain new fields, only these
//! builders need updating — all downstream tests stay intact.

use chrono::Utc;
use data_core::feedback::{CommunitySignal, TaskSignal};
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;

// ── DatasetMetadataBuilder ──────────────────────────────────

pub struct DatasetMetadataBuilder {
    inner: DatasetMetadata,
}

impl DatasetMetadataBuilder {
    pub fn new(cid: &str) -> Self {
        Self {
            inner: DatasetMetadata {
                cid: DatasetCid(cid.into()),
                info_hash: None,
                title: format!("Dataset {cid}"),
                description: Some(format!("{cid} dataset")),
                tags: vec!["test".into()],
                data_type: DataType::Tabular,
                schema: DatasetSchema {
                    columns: vec![],
                    row_count: 1000,
                    size_bytes: 50_000,
                },
                stats: Some(DatasetStats {
                    null_rate: 0.05,
                    unique_rate: 0.8,
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
                provider: Did("did:key:test".into()),
                signature: "sig".into(),
                provenance: Provenance::Original,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                version: None,
                previous_version: None,
                verifiable_credential: None,
                source_attributes: None,
            },
        }
    }

    pub fn title(mut self, t: &str) -> Self {
        self.inner.title = t.into();
        self
    }

    pub fn columns(mut self, cols: &[(&str, &str)]) -> Self {
        self.inner.schema.columns = cols
            .iter()
            .map(|(name, dtype)| ColumnDef {
                name: name.to_string(),
                dtype: dtype.to_string(),
                nullable: false,
                description: Some(format!("{name} column")),
            })
            .collect();
        self
    }

    pub fn price(mut self, amount: f64) -> Self {
        self.inner.price = Price::usdc(amount);
        if amount > 0.0 {
            self.inner.access = AccessMode::Paid;
        }
        self
    }

    pub fn access(mut self, mode: AccessMode) -> Self {
        self.inner.access = mode;
        self
    }

    pub fn stats(mut self, stats: Option<DatasetStats>) -> Self {
        self.inner.stats = stats;
        self
    }

    pub fn vc(mut self, vc: serde_json::Value) -> Self {
        self.inner.verifiable_credential = Some(vc);
        self
    }

    pub fn build(self) -> DatasetMetadata {
        self.inner
    }
}

// ── QualityScoreBuilder ─────────────────────────────────────

pub struct QualityScoreBuilder {
    total: f64,
}

impl QualityScoreBuilder {
    pub fn new(total: f64) -> Self {
        Self { total }
    }

    pub fn build(self) -> QualityScore {
        let t = self.total;
        QualityScore {
            total: t,
            completeness: t,
            consistency: t * 0.9,
            freshness: t * 0.8,
            schema_quality: t * 0.7,
            provenance: 50.0,
            community: 50.0,
        }
    }
}

// ── CommunitySignalBuilder ──────────────────────────────────

pub struct CommunitySignalBuilder {
    cid: String,
    total: u64,
    pos_rate: f64,
    neg_rate: f64,
}

impl CommunitySignalBuilder {
    pub fn new(cid: &str) -> Self {
        Self {
            cid: cid.into(),
            total: 0,
            pos_rate: 0.0,
            neg_rate: 0.0,
        }
    }

    pub fn reviews(mut self, total: u64, pos_rate: f64, neg_rate: f64) -> Self {
        self.total = total;
        self.pos_rate = pos_rate;
        self.neg_rate = neg_rate;
        self
    }

    pub fn build(self) -> CommunitySignal {
        CommunitySignal {
            dataset_cid: DatasetCid(self.cid),
            total_reviews: self.total,
            avg_relevance: 0.7,
            avg_quality: 4.0,
            positive_rate: self.pos_rate,
            negative_rate: self.neg_rate,
            task_signals: if self.total > 0 {
                vec![TaskSignal {
                    task_type: "classification".into(),
                    count: self.total,
                    avg_relevance: 0.7,
                    success_rate: 0.8,
                }]
            } else {
                vec![]
            },
        }
    }
}
