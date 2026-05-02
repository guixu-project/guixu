// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use data_core::metadata::DatasetMetadata;
use data_core::types::DatasetCid;
use serde::{Deserialize, Serialize};

use super::trait_def::SampleRequirements;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingInput {
    pub cid: DatasetCid,
    pub metadata: DatasetMetadata,
    pub task_description: String,
    pub task_type: String,
    pub requirements: SampleRequirements,
    #[serde(default)]
    pub budget_bytes: u64,
    #[serde(default)]
    pub budget_rows: u64,
}

impl SamplingInput {
    pub fn new(
        cid: DatasetCid,
        metadata: DatasetMetadata,
        task_description: String,
        task_type: String,
    ) -> Self {
        Self {
            cid,
            metadata,
            task_description,
            task_type,
            requirements: SampleRequirements::default(),
            budget_bytes: u64::MAX,
            budget_rows: u64::MAX,
        }
    }

    pub fn with_requirements(mut self, requirements: SampleRequirements) -> Self {
        self.requirements = requirements;
        self
    }

    pub fn with_budget(mut self, bytes: u64, rows: u64) -> Self {
        self.budget_bytes = bytes;
        self.budget_rows = rows;
        self
    }

    pub fn cid_str(&self) -> &str {
        &self.cid.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sampling_input_builder() {
        let cid = DatasetCid("test_cid".to_string());
        let metadata = DatasetMetadata {
            cid: cid.clone(),
            info_hash: None,
            title: "Test Dataset".to_string(),
            description: None,
            tags: vec![],
            data_type: data_core::types::DataType::Tabular,
            schema: data_core::types::DatasetSchema {
                columns: vec![],
                row_count: 10000,
                size_bytes: 1024 * 1024,
            },
            stats: None,
            video_meta: None,
            access: data_core::types::AccessMode::Open,
            price: data_core::types::Price {
                amount: 0.0,
                currency: "USD".to_string(),
            },
            license: data_core::types::License {
                spdx_id: "MIT".to_string(),
                commercial_use: true,
                derivative_allowed: true,
            },
            provider: data_core::types::Did("did:example:123".to_string()),
            signature: String::new(),
            provenance: data_core::metadata::Provenance::Original,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            verifiable_credential: None,
            source_attributes: None,
            version: None,
            previous_version: None,
        };

        let input = SamplingInput::new(
            cid,
            metadata,
            "Test task".to_string(),
            "classification".to_string(),
        )
        .with_budget(1024 * 1024, 1000);

        assert_eq!(input.cid_str(), "test_cid");
        assert_eq!(input.budget_rows, 1000);
        assert_eq!(input.budget_bytes, 1024 * 1024);
    }
}
