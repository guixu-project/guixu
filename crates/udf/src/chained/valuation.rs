// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use crate::common::{
    UDFCategory, UDFError, UDFId, UDFLimits, UDFMetadata, UDFResult, UFDCapabilities,
};
use crate::valuation::{ValuationInput, ValuationOutput, ValuationUDF, ValuationVerdict};

#[async_trait]
pub trait UDFRegistryTrait: Send + Sync {
    async fn evaluate_valuation(
        &self,
        id: &UDFId,
        input: &ValuationInput,
    ) -> UDFResult<ValuationOutput>;
    async fn sample_sampling(
        &self,
        id: &UDFId,
        input: &crate::sampling::SamplingInput,
        records: &[crate::sampling::SampleRecord],
    ) -> UDFResult<crate::sampling::SamplingOutput>;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ValuationCombinationMode {
    #[default]
    WeightedSum,
    GeometricMean,
    Max,
    Min,
    RankAverage,
}

pub struct ChainedValuationStage {
    pub udf_id: UDFId,
    pub weight: f64,
    pub condition: Option<Box<dyn StageCondition>>,
}

impl ChainedValuationStage {
    pub fn new(udf_id: UDFId, weight: f64) -> Self {
        Self {
            udf_id,
            weight,
            condition: None,
        }
    }

    pub fn with_condition<C: StageCondition + 'static>(mut self, condition: C) -> Self {
        self.condition = Some(Box::new(condition));
        self
    }
}

pub trait StageCondition: Send + Sync {
    fn matches(&self, input: &ValuationInput) -> bool;
}

pub struct AlwaysTrueCondition;

impl StageCondition for AlwaysTrueCondition {
    fn matches(&self, _input: &ValuationInput) -> bool {
        true
    }
}

pub struct TaskTypeCondition {
    pub task_types: Vec<String>,
}

impl StageCondition for TaskTypeCondition {
    fn matches(&self, input: &ValuationInput) -> bool {
        self.task_types
            .iter()
            .any(|tt| tt == "*" || *tt == input.task_type)
    }
}

pub struct BudgetThresholdCondition {
    pub min_budget: f64,
}

impl StageCondition for BudgetThresholdCondition {
    fn matches(&self, input: &ValuationInput) -> bool {
        input.budget >= self.min_budget
    }
}

pub struct ChainedValuationUDF {
    stages: Vec<ChainedValuationStage>,
    combination_mode: ValuationCombinationMode,
}

impl ChainedValuationUDF {
    pub fn new(
        stages: Vec<ChainedValuationStage>,
        combination_mode: ValuationCombinationMode,
    ) -> Self {
        Self {
            stages,
            combination_mode,
        }
    }

    pub async fn evaluate(
        &self,
        input: &ValuationInput,
        registry: &dyn UDFRegistryTrait,
    ) -> UDFResult<ValuationOutput> {
        let mut scores = Vec::new();
        let mut total_weight = 0.0f64;

        for stage in &self.stages {
            if let Some(ref cond) = stage.condition {
                if !cond.matches(input) {
                    continue;
                }
            }

            match registry.evaluate_valuation(&stage.udf_id, input).await {
                Ok(output) => {
                    scores.push((stage.weight, output.score));
                    total_weight += stage.weight;
                }
                Err(e) => {
                    tracing::warn!("Chained valuation stage '{}' failed: {}", stage.udf_id, e);
                }
            }
        }

        if scores.is_empty() {
            return Err(UDFError::ExecutionError(
                "All chained valuation stages failed".to_string(),
            ));
        }

        let final_score = match self.combination_mode {
            ValuationCombinationMode::WeightedSum => {
                scores.iter().map(|(w, s)| w * s).sum::<f64>() / total_weight
            }
            ValuationCombinationMode::GeometricMean => scores
                .iter()
                .map(|(w, s)| s.powf(*w / total_weight))
                .product::<f64>(),
            ValuationCombinationMode::Max => {
                scores.iter().map(|(_, s)| s).fold(0.0f64, |a, b| a.max(*b))
            }
            ValuationCombinationMode::Min => scores
                .iter()
                .map(|(_, s)| s)
                .fold(100.0f64, |a, b| a.min(*b)),
            ValuationCombinationMode::RankAverage => {
                let mut sorted = scores.iter().enumerate().collect::<Vec<_>>();
                sorted.sort_by(|a, b| a.1 .1.partial_cmp(&b.1 .1).unwrap());
                let avg_rank = sorted
                    .iter()
                    .enumerate()
                    .map(|(i, (_, (w, _)))| (i as f64 + 1.0) * w)
                    .sum::<f64>()
                    / total_weight;
                avg_rank * 100.0 / scores.len() as f64
            }
        };

        Ok(ValuationOutput {
            score: final_score,
            verdict: ValuationVerdict::from_score(final_score),
            breakdown: vec![],
            explanation: format!(
                "Chained evaluation of {} UDFs using {:?}",
                scores.len(),
                self.combination_mode
            ),
            metadata: serde_json::json!({
                "stages": scores.iter().map(|(w, s)| serde_json::json!({"weight": w, "score": s})).collect::<Vec<_>>(),
                "combination_mode": self.combination_mode,
            }),
        })
    }
}

#[async_trait]
impl ValuationUDF for ChainedValuationUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: LazyLock<UDFMetadata, fn() -> UDFMetadata> =
            LazyLock::new(|| UDFMetadata {
                id: UDFId("builtin:valuation:chained".into()),
                category: UDFCategory::Valuation,
                name: "Chained Valuation UDF".into(),
                version: "1.0.0".into(),
                author: "Guixu".into(),
                description:
                    "Chains multiple valuation UDFs together with configurable combination modes."
                        .into(),
                tags: vec!["builtin".into(), "chained".into()],
                parameters: vec![],
                capabilities: UFDCapabilities::default(),
                limits: UDFLimits::default(),
            });
        &METADATA
    }

    async fn evaluate(&self, _input: &ValuationInput) -> UDFResult<ValuationOutput> {
        Err(UDFError::ExecutionError(
            "ChainedValuationUDF::evaluate requires a registry reference. Use evaluate_with_registry.".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combination_modes() {
        let mode = ValuationCombinationMode::WeightedSum;
        assert_eq!(format!("{:?}", mode), "WeightedSum");

        let mode = ValuationCombinationMode::GeometricMean;
        assert_eq!(format!("{:?}", mode), "GeometricMean");
    }

    #[test]
    fn test_chained_stage_builder() {
        let stage = ChainedValuationStage::new(UDFId::new("builtin:valuation:tcv"), 0.5);
        assert_eq!(stage.weight, 0.5);
        assert!(stage.condition.is_none());

        let stage = stage.with_condition(TaskTypeCondition {
            task_types: vec!["classification".to_string()],
        });
        assert!(stage.condition.is_some());
    }
}
