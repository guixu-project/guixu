use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use data_core::types::QualityScore;

/// Computes the universal quality score (0-100) for any dataset.
pub struct QualityScorer;

impl QualityScorer {
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

        let community = 50.0; // TODO(milestone-3): aggregate from EAS attestations

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

    /// Score with actual data access (more accurate, requires download).
    pub fn score_from_data(
        &self,
        _data: &[u8],
        metadata: &DatasetMetadata,
    ) -> Result<QualityScore> {
        // TODO(milestone-2): use Polars to compute actual stats
        Ok(self.score_from_metadata(metadata))
    }
}
