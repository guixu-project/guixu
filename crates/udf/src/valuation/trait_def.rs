// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use crate::common::{UDFCategory, UDFId, UDFLimits, UDFMetadata, UDFResult, UFDCapabilities};

use super::input::ValuationInput;
use super::output::ValuationOutput;

pub const UDF_ID_NOOP: &str = "builtin:valuation:noop";
pub const UDF_ID_TCV: &str = "builtin:valuation:tcv";
pub const UDF_ID_FREE: &str = "builtin:valuation:free";
pub const UDF_ID_PAID: &str = "builtin:valuation:paid";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValuationVerdict {
    StrongPositive,
    Positive,
    Neutral,
    Negative,
    StrongNegative,
}

impl ValuationVerdict {
    pub fn from_score(score: f64) -> Self {
        match score {
            s if s > 80.0 => ValuationVerdict::StrongPositive,
            s if s > 65.0 => ValuationVerdict::Positive,
            s if s > 50.0 => ValuationVerdict::Neutral,
            s if s > 35.0 => ValuationVerdict::Negative,
            _ => ValuationVerdict::StrongNegative,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ValuationVerdict::StrongPositive => "strongly recommended",
            ValuationVerdict::Positive => "recommended",
            ValuationVerdict::Neutral => "marginal value",
            ValuationVerdict::Negative => "likely unhelpful",
            ValuationVerdict::StrongNegative => "would likely harm task",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreComponent {
    pub id: String,
    pub label: String,
    pub value: f64,
    pub weight: f64,
    pub contribution: f64,
}

impl ScoreComponent {
    pub fn new(id: impl Into<String>, label: impl Into<String>, value: f64, weight: f64) -> Self {
        let contribution = weight * value;
        Self {
            id: id.into(),
            label: label.into(),
            value,
            weight,
            contribution,
        }
    }

    pub fn compute_contribution(&mut self) {
        self.contribution = self.weight * self.value;
    }
}

#[async_trait]
pub trait ValuationUDF: Send + Sync {
    fn metadata(&self) -> &UDFMetadata;

    async fn evaluate(&self, input: &ValuationInput) -> UDFResult<ValuationOutput>;

    fn is_deterministic(&self) -> bool {
        true
    }

    fn supported_task_types(&self) -> Vec<String> {
        vec!["*".to_string()]
    }
}

pub trait BuiltinValuationUDF {
    const ID: &'static str;
    fn create(config: serde_json::Value) -> UDFResult<Box<dyn ValuationUDF>>
    where
        Self: Sized;
}

pub struct NoOpValuationUDF;

#[async_trait]
impl ValuationUDF for NoOpValuationUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: LazyLock<UDFMetadata, fn() -> UDFMetadata> =
            LazyLock::new(|| UDFMetadata {
                id: UDFId(UDF_ID_NOOP.into()),
                category: UDFCategory::Valuation,
                name: "No-Op Valuation".into(),
                version: "1.0.0".into(),
                author: "Guixu".into(),
                description: "Dummy valuation UDF that returns neutral scores".into(),
                tags: vec![],
                parameters: vec![],
                capabilities: UFDCapabilities::default(),
                limits: UDFLimits::default(),
            });
        &METADATA
    }

    async fn evaluate(&self, _input: &ValuationInput) -> UDFResult<ValuationOutput> {
        Ok(ValuationOutput {
            score: 50.0,
            verdict: ValuationVerdict::Neutral,
            breakdown: vec![],
            explanation: "No-op evaluation: neutral score".to_string(),
            metadata: serde_json::Value::Null,
        })
    }
}
