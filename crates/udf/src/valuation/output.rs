// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use super::trait_def::{ScoreComponent, ValuationVerdict};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValuationOutput {
    pub score: f64,
    pub verdict: ValuationVerdict,
    #[serde(default)]
    pub breakdown: Vec<ScoreComponent>,
    pub explanation: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl ValuationOutput {
    pub fn new(score: f64, explanation: impl Into<String>) -> Self {
        Self {
            score,
            verdict: ValuationVerdict::from_score(score),
            breakdown: vec![],
            explanation: explanation.into(),
            metadata: serde_json::Value::Null,
        }
    }

    pub fn with_breakdown(mut self, breakdown: Vec<ScoreComponent>) -> Self {
        self.breakdown = breakdown;
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn score_clamped(&self) -> f64 {
        self.score.clamp(0.0, 100.0)
    }

    pub fn total_contribution(&self) -> f64 {
        self.breakdown.iter().map(|c| c.contribution).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valuation_output_builder() {
        let output = ValuationOutput::new(75.0, "Test explanation").with_breakdown(vec![
            ScoreComponent::new("schema", "Schema Fit", 80.0, 0.25),
            ScoreComponent::new("quality", "Quality", 70.0, 0.10),
        ]);

        assert_eq!(output.score, 75.0);
        assert_eq!(output.verdict, ValuationVerdict::Positive);
        assert_eq!(output.breakdown.len(), 2);
        assert_eq!(output.total_contribution(), 27.0);
    }

    #[test]
    fn test_verdict_from_score() {
        assert_eq!(
            ValuationVerdict::from_score(85.0),
            ValuationVerdict::StrongPositive
        );
        assert_eq!(
            ValuationVerdict::from_score(70.0),
            ValuationVerdict::Positive
        );
        assert_eq!(
            ValuationVerdict::from_score(55.0),
            ValuationVerdict::Neutral
        );
        assert_eq!(
            ValuationVerdict::from_score(40.0),
            ValuationVerdict::Negative
        );
        assert_eq!(
            ValuationVerdict::from_score(30.0),
            ValuationVerdict::StrongNegative
        );
    }
}
