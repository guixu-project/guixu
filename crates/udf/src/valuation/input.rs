// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use data_core::metadata::DatasetMetadata;
use data_core::types::DatasetCid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValuationInput {
    pub cid: DatasetCid,
    pub metadata: DatasetMetadata,
    pub task_description: String,
    pub task_type: String,
    #[serde(default)]
    pub required_columns: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range: Option<(String, String)>,
    #[serde(default)]
    pub existing_data_cids: Vec<String>,
    #[serde(default)]
    pub budget: f64,
    #[serde(default)]
    pub context: serde_json::Value,
}

impl ValuationInput {
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
            required_columns: vec![],
            time_range: None,
            existing_data_cids: vec![],
            budget: 0.0,
            context: serde_json::Value::Null,
        }
    }

    pub fn with_required_columns(mut self, columns: Vec<String>) -> Self {
        self.required_columns = columns;
        self
    }

    pub fn with_time_range(mut self, start: &str, end: &str) -> Self {
        self.time_range = Some((start.to_string(), end.to_string()));
        self
    }

    pub fn with_existing_data(mut self, cids: Vec<DatasetCid>) -> Self {
        self.existing_data_cids = cids.iter().map(|c| c.0.clone()).collect();
        self
    }

    pub fn with_budget(mut self, budget: f64) -> Self {
        self.budget = budget;
        self
    }

    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = context;
        self
    }

    pub fn cid_str(&self) -> &str {
        &self.cid.0
    }

    pub fn schema_column_names(&self) -> Vec<String> {
        self.metadata
            .schema
            .columns
            .iter()
            .map(|c| c.name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valuation_input_builder() {
        let cid = DatasetCid("test_cid".to_string());
        let metadata = DatasetMetadata {
            cid: cid.clone(),
            info_hash: None,
            title: "Test Dataset".to_string(),
            description: None,
            tags: vec![],
            data_type: data_core::types::DataType::Tabular,
            schema: data_core::types::DatasetSchema {
                columns: vec![
                    data_core::types::ColumnDef {
                        name: "col1".to_string(),
                        dtype: "unknown".to_string(),
                        nullable: true,
                        description: None,
                    },
                    data_core::types::ColumnDef {
                        name: "col2".to_string(),
                        dtype: "unknown".to_string(),
                        nullable: true,
                        description: None,
                    },
                ],
                row_count: 1000,
                size_bytes: 1024,
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

        let input = ValuationInput::new(
            cid,
            metadata,
            "Test task".to_string(),
            "classification".to_string(),
        )
        .with_required_columns(vec!["col1".to_string()])
        .with_time_range("2024-01-01", "2024-12-31")
        .with_budget(100.0);

        assert_eq!(input.cid_str(), "test_cid");
        assert_eq!(input.task_type, "classification");
        assert_eq!(input.required_columns, vec!["col1"]);
        assert_eq!(
            input.time_range,
            Some(("2024-01-01".to_string(), "2024-12-31".to_string()))
        );
        assert_eq!(input.schema_column_names(), vec!["col1", "col2"]);
    }
}
