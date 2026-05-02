// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use crate::common::{
    UDFCategory, UDFError, UDFId, UDFLimits, UDFMetadata, UDFResult, UFDCapabilities,
};
use crate::sampling::{SampleRecord, SamplingInput, SamplingOutput, SamplingUDF};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SamplingCombinationMode {
    #[default]
    Concat,
    Union,
    Intersect,
    FirstNonEmpty,
}

pub struct ChainedSamplingStage {
    pub udf_id: UDFId,
    pub pass_through: bool,
    pub condition: Option<Box<dyn SamplingStageCondition>>,
}

impl ChainedSamplingStage {
    pub fn new(udf_id: UDFId) -> Self {
        Self {
            udf_id,
            pass_through: false,
            condition: None,
        }
    }

    pub fn with_pass_through(mut self, pass_through: bool) -> Self {
        self.pass_through = pass_through;
        self
    }

    pub fn with_condition<C: SamplingStageCondition + 'static>(mut self, condition: C) -> Self {
        self.condition = Some(Box::new(condition));
        self
    }
}

pub trait SamplingStageCondition: Send + Sync {
    fn matches(&self, input: &SamplingInput) -> bool;
}

pub struct DataTypeCondition {
    pub data_types: Vec<String>,
}

impl SamplingStageCondition for DataTypeCondition {
    fn matches(&self, input: &SamplingInput) -> bool {
        let dt = format!("{:?}", input.metadata.data_type).to_lowercase();
        self.data_types
            .iter()
            .any(|d| d == "*" || d.to_lowercase() == dt)
    }
}

pub struct BudgetRangeCondition {
    pub min_budget_bytes: u64,
    pub max_budget_bytes: u64,
}

impl SamplingStageCondition for BudgetRangeCondition {
    fn matches(&self, input: &SamplingInput) -> bool {
        input.budget_bytes >= self.min_budget_bytes && input.budget_bytes <= self.max_budget_bytes
    }
}

pub struct ChainedSamplingUDF {
    stages: Vec<ChainedSamplingStage>,
    combination_mode: SamplingCombinationMode,
}

impl ChainedSamplingUDF {
    pub fn new(
        stages: Vec<ChainedSamplingStage>,
        combination_mode: SamplingCombinationMode,
    ) -> Self {
        Self {
            stages,
            combination_mode,
        }
    }

    pub async fn sample_with_registry<R>(
        &self,
        input: &SamplingInput,
        all_records: &[SampleRecord],
        registry: &R,
    ) -> UDFResult<SamplingOutput>
    where
        R: crate::chained::valuation::UDFRegistryTrait,
    {
        let mut stage_outputs: Vec<Vec<SampleRecord>> = Vec::new();

        for stage in &self.stages {
            if let Some(ref cond) = stage.condition {
                if !cond.matches(input) {
                    continue;
                }
            }

            match registry
                .sample_sampling(&stage.udf_id, input, all_records)
                .await
            {
                Ok(output) => {
                    if stage.pass_through {
                        return Ok(output);
                    }
                    stage_outputs.push(output.selected_records);
                }
                Err(e) => {
                    tracing::warn!("Chained sampling stage '{}' failed: {}", stage.udf_id, e);
                }
            }
        }

        if stage_outputs.is_empty() {
            return Err(UDFError::ExecutionError(
                "All chained sampling stages failed".to_string(),
            ));
        }

        let stages_count = stage_outputs.len();
        let combined = match self.combination_mode {
            SamplingCombinationMode::Concat => {
                let mut combined = Vec::new();
                for output in stage_outputs {
                    combined.extend(output);
                }
                combined
            }
            SamplingCombinationMode::Union => {
                let mut seen = std::collections::HashSet::new();
                let mut combined = Vec::new();
                for output in stage_outputs {
                    for record in output {
                        if seen.insert(record.index) {
                            combined.push(record);
                        }
                    }
                }
                combined
            }
            SamplingCombinationMode::Intersect => {
                if stage_outputs.len() == 1 {
                    stage_outputs.into_iter().next().unwrap_or_default()
                } else {
                    let mut intersection = stage_outputs[0].clone();
                    for output in stage_outputs.iter().skip(1) {
                        let output_indices: std::collections::HashSet<_> =
                            output.iter().map(|r| r.index).collect();
                        intersection.retain(|r| output_indices.contains(&r.index));
                    }
                    intersection
                }
            }
            SamplingCombinationMode::FirstNonEmpty => {
                stage_outputs.into_iter().next().unwrap_or_default()
            }
        };

        let sampled_rows = combined.len() as u64;
        Ok(SamplingOutput {
            selected_records: combined,
            sampled_bytes: 0,
            sampled_rows,
            explanation: format!(
                "Chained sampling: {} stages, {} samples using {:?}",
                stages_count, sampled_rows, self.combination_mode
            ),
            metadata: serde_json::json!({
                "stages_count": stages_count,
                "combination_mode": self.combination_mode,
            }),
        })
    }
}

#[async_trait]
impl SamplingUDF for ChainedSamplingUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: LazyLock<UDFMetadata, fn() -> UDFMetadata> =
            LazyLock::new(|| UDFMetadata {
                id: UDFId("builtin:sampling:chained".into()),
                category: UDFCategory::Sampling,
                name: "Chained Sampling UDF".into(),
                version: "1.0.0".into(),
                author: "Guixu".into(),
                description:
                    "Chains multiple sampling UDFs together with configurable combination modes."
                        .into(),
                tags: vec!["builtin".into(), "chained".into()],
                parameters: vec![],
                capabilities: UFDCapabilities::default(),
                limits: UDFLimits::default(),
            });
        &METADATA
    }

    async fn sample(
        &self,
        _input: &SamplingInput,
        _all_records: &[SampleRecord],
    ) -> UDFResult<SamplingOutput> {
        Err(UDFError::ExecutionError(
            "ChainedSamplingUDF::sample requires a registry reference. Use sample_with_registry."
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combination_modes() {
        let mode = SamplingCombinationMode::Concat;
        assert_eq!(format!("{:?}", mode), "Concat");

        let mode = SamplingCombinationMode::Intersect;
        assert_eq!(format!("{:?}", mode), "Intersect");
    }

    #[test]
    fn test_chained_sampling_stage_builder() {
        let stage = ChainedSamplingStage::new(UDFId::new("builtin:sampling:random"));
        assert!(stage.condition.is_none());
        assert!(!stage.pass_through);

        let stage = stage
            .with_pass_through(true)
            .with_condition(DataTypeCondition {
                data_types: vec!["tabular".to_string()],
            });
        assert!(stage.pass_through);
        assert!(stage.condition.is_some());
    }
}
