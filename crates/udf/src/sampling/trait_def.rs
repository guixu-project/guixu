// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use crate::common::{UDFCategory, UDFId, UDFLimits, UDFMetadata, UDFResult, UFDCapabilities};

use super::input::SamplingInput;
use super::output::SamplingOutput;

pub const UDF_ID_NOOP_SAMPLING: &str = "builtin:sampling:noop";
pub const UDF_ID_RANDOM: &str = "builtin:sampling:random";
pub const UDF_ID_STRATIFIED: &str = "builtin:sampling:stratified";
pub const UDF_ID_KNN_SHAPLEY: &str = "builtin:sampling:knn_shapley";
pub const UDF_ID_LABEL_PROPAGATION: &str = "builtin:sampling:label_propagation";
pub const UDF_ID_STAGED: &str = "builtin:sampling:staged";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleRecord {
    pub index: usize,
    pub data: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub label: Option<String>,
}

impl SampleRecord {
    pub fn new(index: usize, data: serde_json::Value) -> Self {
        Self {
            index,
            data,
            metadata: serde_json::Value::Null,
            score: None,
            label: None,
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn with_score(mut self, score: f64) -> Self {
        self.score = Some(score);
        self
    }

    pub fn with_label(mut self, label: String) -> Self {
        self.label = Some(label);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SampleRequirements {
    pub summary: String,
    #[serde(default)]
    pub required_signals: Vec<String>,
    #[serde(default)]
    pub preferred_labels: Vec<String>,
    #[serde(default)]
    pub disqualifying_signals: Vec<String>,
}

impl SampleRequirements {
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            required_signals: vec![],
            preferred_labels: vec![],
            disqualifying_signals: vec![],
        }
    }

    pub fn with_required_signals(mut self, signals: Vec<String>) -> Self {
        self.required_signals = signals;
        self
    }

    pub fn with_preferred_labels(mut self, labels: Vec<String>) -> Self {
        self.preferred_labels = labels;
        self
    }

    pub fn is_satisfied_by(&self, record: &SampleRecord) -> bool {
        for signal in &self.disqualifying_signals {
            let signal_lower = signal.to_lowercase();
            let data_str = record.data.to_string().to_lowercase();
            if data_str.contains(&signal_lower) {
                return false;
            }
        }
        true
    }
}

#[async_trait]
pub trait SamplingUDF: Send + Sync {
    fn metadata(&self) -> &UDFMetadata;

    async fn sample(
        &self,
        input: &SamplingInput,
        all_records: &[SampleRecord],
    ) -> UDFResult<SamplingOutput>;

    fn compute_requirements(&self, task_description: &str) -> SampleRequirements {
        let _ = task_description;
        SampleRequirements::default()
    }

    fn is_deterministic(&self) -> bool {
        false
    }

    fn supported_task_types(&self) -> Vec<String> {
        vec!["*".to_string()]
    }
}

pub trait BuiltinSamplingUDF {
    const ID: &'static str;
    fn create(config: serde_json::Value) -> UDFResult<Box<dyn SamplingUDF>>
    where
        Self: Sized;
}

pub struct NoOpSamplingUDF;

#[async_trait]
impl SamplingUDF for NoOpSamplingUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: LazyLock<UDFMetadata, fn() -> UDFMetadata> =
            LazyLock::new(|| UDFMetadata {
                id: UDFId(UDF_ID_NOOP_SAMPLING.into()),
                category: UDFCategory::Sampling,
                name: "No-Op Sampling".into(),
                version: "1.0.0".into(),
                author: "Guixu".into(),
                description: "Dummy sampling UDF that returns all records".into(),
                tags: vec![],
                parameters: vec![],
                capabilities: UFDCapabilities::default(),
                limits: UDFLimits::default(),
            });
        &METADATA
    }

    async fn sample(
        &self,
        input: &SamplingInput,
        all_records: &[SampleRecord],
    ) -> UDFResult<SamplingOutput> {
        let selected = all_records
            .iter()
            .take(input.budget_rows as usize)
            .cloned()
            .collect();
        Ok(SamplingOutput {
            selected_records: selected,
            sampled_bytes: 0,
            sampled_rows: all_records.len().min(input.budget_rows as usize) as u64,
            explanation: "No-op sampling: returned all records within budget".to_string(),
            metadata: serde_json::Value::Null,
        })
    }
}
