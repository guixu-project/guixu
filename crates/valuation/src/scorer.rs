// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use data_core::types::QualityScore;
use data_storage::feedback_store::FeedbackStore;
use std::sync::Arc;

/// Computes the universal quality score (0-100) for any dataset.
pub struct QualityScorer {
    feedback_store: Option<Arc<FeedbackStore>>,
}

impl QualityScorer {
    /// Create a new QualityScorer without feedback store (community score defaults to 50.0).
    pub fn new() -> Self {
        Self {
            feedback_store: None,
        }
    }

    /// Create a QualityScorer with a feedback store for community scoring.
    pub fn with_feedback_store(store: Arc<FeedbackStore>) -> Self {
        Self {
            feedback_store: Some(store),
        }
    }

    /// Score a dataset based on its metadata (no data access needed).
    pub fn score_from_metadata(&self, metadata: &DatasetMetadata) -> QualityScore {
        let completeness = metadata
            .stats
            .as_ref()
            .map(|s| (1.0 - s.null_rate) * 100.0)
            .unwrap_or(50.0);

        let consistency = metadata
            .stats
            .as_ref()
            .map(|s| s.unique_rate * 100.0)
            .unwrap_or(50.0);

        let freshness = {
            let age_days = (chrono::Utc::now() - metadata.updated_at).num_days() as f64;
            (100.0 - age_days * 0.5).max(0.0)
        };

        let schema_quality = {
            let has_desc = metadata.description.is_some() as u8 as f64;
            let has_types = (!metadata.schema.columns.is_empty()) as u8 as f64;
            let has_col_desc = metadata
                .schema
                .columns
                .iter()
                .any(|c| c.description.is_some()) as u8 as f64;
            (has_desc + has_types + has_col_desc) / 3.0 * 100.0
        };

        let provenance = if metadata.verifiable_credential.is_some() {
            80.0
        } else if !metadata.signature.is_empty() {
            50.0
        } else {
            20.0
        };

        let community = self.compute_community_score(metadata);

        let total = completeness * 0.25
            + consistency * 0.20
            + freshness * 0.20
            + schema_quality * 0.15
            + provenance * 0.10
            + community * 0.10;

        QualityScore {
            total,
            completeness,
            consistency,
            freshness,
            schema_quality,
            provenance,
            community,
        }
    }

    /// Compute community score from EAS attestations via FeedbackStore.
    fn compute_community_score(&self, metadata: &DatasetMetadata) -> f64 {
        let Some(store) = &self.feedback_store else {
            return 50.0;
        };

        match store.compute_signal(&metadata.cid) {
            Ok(signal) => signal.score_for_task("default"),
            Err(_) => 50.0,
        }
    }

    /// Score with actual data access (more accurate, requires download).
    pub fn score_from_data(&self, data: &[u8], metadata: &DatasetMetadata) -> Result<QualityScore> {
        use polars::prelude::*;

        // Try to parse data as Parquet or CSV to compute actual stats
        let parsed = if data.starts_with(&[0x50, 0x41, 0x52, 0x23]) {
            // Parquet magic bytes: "PAR1"
            let reader = ParquetReader::new(std::io::Cursor::new(data));
            reader.finish().ok()
        } else if data.starts_with(&[0xff, 0xfe]) || data.starts_with(&[0xff, 0xf9]) {
            // CSV BOM (utf-8 BOM or utf-16 LE/BE)
            CsvReader::new(std::io::Cursor::new(data)).finish().ok()
        } else {
            // Try as CSV without BOM
            CsvReader::new(std::io::Cursor::new(data)).finish().ok()
        };

        let mut score = self.score_from_metadata(metadata);

        if let Some(df) = parsed {
            let height = df.height() as f64;
            if height > 0.0 {
                // Compute actual null_rate from DataFrame
                let null_count: u64 = df
                    .null_count()
                    .iter()
                    .map(|s| s.sum::<u64>().unwrap_or(0))
                    .sum();
                let null_rate = null_count as f64 / (height * df.width() as f64);
                score.completeness = (1.0 - null_rate) * 100.0;

                // Compute actual unique_rate
                let total_unique: u64 = df.iter().map(|s| s.n_unique().unwrap_or(1) as u64).sum();
                let unique_rate = (total_unique as f64 / height).min(1.0);
                score.consistency = unique_rate * 100.0;
            }
        }

        Ok(score)
    }
}

impl Default for QualityScorer {
    fn default() -> Self {
        Self::new()
    }
}
