// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use super::trait_def::SampleRecord;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingOutput {
    #[serde(default)]
    pub selected_records: Vec<SampleRecord>,
    #[serde(default)]
    pub sampled_bytes: u64,
    #[serde(default)]
    pub sampled_rows: u64,
    pub explanation: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl SamplingOutput {
    pub fn new(explanation: impl Into<String>) -> Self {
        Self {
            selected_records: vec![],
            sampled_bytes: 0,
            sampled_rows: 0,
            explanation: explanation.into(),
            metadata: serde_json::Value::Null,
        }
    }

    pub fn with_records(mut self, records: Vec<SampleRecord>) -> Self {
        self.selected_records = records;
        self.sampled_rows = self.selected_records.len() as u64;
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.selected_records.is_empty()
    }

    pub fn len(&self) -> usize {
        self.selected_records.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sampling_output_builder() {
        let records = vec![
            SampleRecord::new(0, serde_json::json!({"id": 1})),
            SampleRecord::new(1, serde_json::json!({"id": 2})),
        ];

        let output = SamplingOutput::new("Test sampling").with_records(records);

        assert_eq!(output.len(), 2);
        assert_eq!(output.sampled_rows, 2);
        assert!(!output.is_empty());
    }
}
