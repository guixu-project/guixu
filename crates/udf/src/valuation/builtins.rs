// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::LazyLock;

use async_trait::async_trait;

use crate::common::{
    ParameterType, UDFCategory, UDFId, UDFLimits, UDFMetadata, UDFParameter, UFDCapabilities,
};
use crate::valuation::{ValuationInput, ValuationOutput, ValuationUDF, ValuationVerdict};

pub struct TcvUDFConfig {
    pub weights: TcvWeights,
    pub include_community_signal: bool,
    pub include_risk_penalty: bool,
}

#[derive(Debug, Clone)]
pub struct TcvWeights {
    pub schema_fit: f64,
    pub temporal_fit: f64,
    pub information_gain: f64,
    pub quality: f64,
    pub community_signal: f64,
    pub risk_penalty: f64,
}

impl Default for TcvWeights {
    fn default() -> Self {
        Self {
            schema_fit: 0.25,
            temporal_fit: 0.15,
            information_gain: 0.15,
            quality: 0.10,
            community_signal: 0.15,
            risk_penalty: 0.20,
        }
    }
}

impl TcvWeights {
    pub fn validate(&self) -> bool {
        let sum = self.schema_fit
            + self.temporal_fit
            + self.information_gain
            + self.quality
            + self.community_signal
            + self.risk_penalty;
        (sum - 1.0).abs() < 0.001
    }
}

pub struct TcvUDF {
    config: TcvUDFConfig,
}

impl TcvUDF {
    pub fn new(config: TcvUDFConfig) -> Self {
        Self { config }
    }

    pub fn from_json(config: serde_json::Value) -> Result<Self, String> {
        let weights = TcvWeights {
            schema_fit: config
                .get("weights")
                .and_then(|w| w.get("schema_fit"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.25),
            temporal_fit: config
                .get("weights")
                .and_then(|w| w.get("temporal_fit"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.15),
            information_gain: config
                .get("weights")
                .and_then(|w| w.get("information_gain"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.15),
            quality: config
                .get("weights")
                .and_then(|w| w.get("quality"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.10),
            community_signal: config
                .get("weights")
                .and_then(|w| w.get("community_signal"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.15),
            risk_penalty: config
                .get("weights")
                .and_then(|w| w.get("risk_penalty"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.20),
        };

        let include_community_signal = config
            .get("include_community_signal")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let include_risk_penalty = config
            .get("include_risk_penalty")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        Ok(Self {
            config: TcvUDFConfig {
                weights,
                include_community_signal,
                include_risk_penalty,
            },
        })
    }
}

#[async_trait]
impl ValuationUDF for TcvUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: LazyLock<UDFMetadata, fn() -> UDFMetadata> = LazyLock::new(|| {
            UDFMetadata {
            id: UDFId(crate::valuation::UDF_ID_TCV.into()),
            category: UDFCategory::Valuation,
            name: "Task-Conditioned Value (TCV)".into(),
            version: "1.0.0".into(),
            author: "Guixu Team".into(),
            description: "Standard TCV evaluator combining schema fit, temporal coverage, information gain, quality, community signal, and risk penalty.".into(),
            tags: vec!["builtin".into(), "tcv".into(), "standard".into()],
            parameters: vec![
                UDFParameter {
                    name: "weights.schema_fit".into(),
                    param_type: ParameterType::Number,
                    required: false,
                    default: Some(serde_json::json!(0.25)),
                    description: "Weight for schema fit component".into(),
                },
                UDFParameter {
                    name: "weights.temporal_fit".into(),
                    param_type: ParameterType::Number,
                    required: false,
                    default: Some(serde_json::json!(0.15)),
                    description: "Weight for temporal fit component".into(),
                },
                UDFParameter {
                    name: "include_community_signal".into(),
                    param_type: ParameterType::Boolean,
                    required: false,
                    default: Some(serde_json::json!(true)),
                    description: "Whether to include community signal in evaluation".into(),
                },
            ],
            capabilities: UFDCapabilities::default(),
            limits: UDFLimits::default(),
        }
        });
        &METADATA
    }

    async fn evaluate(
        &self,
        input: &ValuationInput,
    ) -> Result<ValuationOutput, crate::common::UDFError> {
        let w = &self.config.weights;

        let schema_fit = self.compute_schema_fit(input);
        let temporal_fit = self.compute_temporal_fit(input);
        let information_gain = self.compute_information_gain(input);
        let quality = self.compute_quality(input);

        let community_signal = if self.config.include_community_signal {
            50.0
        } else {
            0.0
        };

        let risk_penalty = if self.config.include_risk_penalty {
            20.0
        } else {
            0.0
        };

        let raw = w.schema_fit * schema_fit
            + w.temporal_fit * temporal_fit
            + w.information_gain * information_gain
            + w.quality * quality
            + w.community_signal * community_signal
            - w.risk_penalty * risk_penalty;

        let score = ((raw + 100.0) / 2.0).clamp(0.0, 100.0);

        let breakdown = vec![
            crate::valuation::ScoreComponent::new(
                "schema_fit",
                "Schema Fit",
                schema_fit,
                w.schema_fit,
            ),
            crate::valuation::ScoreComponent::new(
                "temporal_fit",
                "Temporal Fit",
                temporal_fit,
                w.temporal_fit,
            ),
            crate::valuation::ScoreComponent::new(
                "information_gain",
                "Information Gain",
                information_gain,
                w.information_gain,
            ),
            crate::valuation::ScoreComponent::new("quality", "Quality", quality, w.quality),
            crate::valuation::ScoreComponent::new(
                "community_signal",
                "Community Signal",
                community_signal,
                w.community_signal,
            ),
            crate::valuation::ScoreComponent::new(
                "risk_penalty",
                "Risk Penalty",
                risk_penalty,
                w.risk_penalty,
            ),
        ];

        Ok(ValuationOutput {
            score,
            verdict: ValuationVerdict::from_score(score),
            breakdown,
            explanation: format!("TCV evaluation for dataset {}", input.cid_str()),
            metadata: serde_json::json!({
                "weights": {
                    "schema_fit": w.schema_fit,
                    "temporal_fit": w.temporal_fit,
                    "information_gain": w.information_gain,
                    "quality": w.quality,
                    "community_signal": w.community_signal,
                    "risk_penalty": w.risk_penalty,
                }
            }),
        })
    }

    fn is_deterministic(&self) -> bool {
        true
    }

    fn supported_task_types(&self) -> Vec<String> {
        vec!["*".to_string()]
    }
}

impl TcvUDF {
    fn compute_schema_fit(&self, input: &ValuationInput) -> f64 {
        if input.required_columns.is_empty() {
            return 50.0;
        }
        let dataset_cols: Vec<String> = input
            .metadata
            .schema
            .columns
            .iter()
            .map(|c| c.name.to_lowercase())
            .collect();
        let matched = input
            .required_columns
            .iter()
            .filter(|rc| {
                let rc_lower = rc.to_lowercase();
                dataset_cols
                    .iter()
                    .any(|dc| dc.contains(&rc_lower) || rc_lower.contains(dc))
            })
            .count();
        (matched as f64 / input.required_columns.len() as f64) * 100.0
    }

    fn compute_temporal_fit(&self, input: &ValuationInput) -> f64 {
        if let Some((start, end)) = &input.time_range {
            let all_text = format!(
                "{} {} {}",
                input.metadata.title,
                input.metadata.description.as_deref().unwrap_or(""),
                input.metadata.tags.join(" ")
            )
            .to_lowercase();
            let has_start = all_text.contains(&start.to_lowercase());
            let has_end = all_text.contains(&end.to_lowercase());
            match (has_start, has_end) {
                (true, true) => 100.0,
                (true, false) | (false, true) => 60.0,
                (false, false) => 30.0,
            }
        } else {
            50.0
        }
    }

    fn compute_information_gain(&self, input: &ValuationInput) -> f64 {
        if input.existing_data_cids.is_empty() {
            return 100.0;
        }
        if input.existing_data_cids.contains(&input.cid.0) {
            return 0.0;
        }
        if input.required_columns.is_empty() {
            let novelty = 1.0 / (1.0 + input.existing_data_cids.len() as f64);
            return (novelty * 100.0).min(100.0);
        }
        let dataset_cols: Vec<String> = input
            .metadata
            .schema
            .columns
            .iter()
            .map(|c| c.name.to_lowercase())
            .collect();
        let mut shapley_sum: f64 = 0.0;
        for (rank, req) in input.required_columns.iter().enumerate() {
            let req_lower = req.to_lowercase();
            let best_match = dataset_cols
                .iter()
                .map(|dc| {
                    if dc == &req_lower {
                        1.0
                    } else if dc.contains(&req_lower) || req_lower.contains(dc) {
                        0.6
                    } else {
                        0.0
                    }
                })
                .fold(0.0_f64, f64::max);
            let weight = 1.0 / (1.0 + rank as f64);
            shapley_sum += best_match * weight;
        }
        let harmonic_n: f64 = (1..=input.required_columns.len())
            .map(|k| 1.0 / k as f64)
            .sum();
        let base_score = (shapley_sum / harmonic_n) * 100.0;
        let redundancy_discount = 1.0 / (1.0 + input.existing_data_cids.len() as f64 * 0.3);
        (base_score * redundancy_discount).clamp(0.0, 100.0)
    }

    fn compute_quality(&self, input: &ValuationInput) -> f64 {
        let completeness = input
            .metadata
            .stats
            .as_ref()
            .map(|s| (1.0 - s.null_rate) * 100.0)
            .unwrap_or(50.0);
        let freshness = {
            let age = (chrono::Utc::now() - input.metadata.updated_at).num_days() as f64;
            (100.0 - age * 0.5).max(0.0)
        };
        let has_schema = (!input.metadata.schema.columns.is_empty()) as u8 as f64 * 100.0;
        completeness * 0.4 + freshness * 0.3 + has_schema * 0.3
    }
}

pub struct FreeEvaluatorUDFConfig {
    pub weights: FreeEvaluatorWeights,
}

#[derive(Debug, Clone)]
pub struct FreeEvaluatorWeights {
    pub schema_relevance: f64,
    pub temporal_coverage: f64,
    pub information_gain: f64,
    pub data_quality: f64,
    pub freshness: f64,
    pub dedup_value: f64,
}

impl Default for FreeEvaluatorWeights {
    fn default() -> Self {
        Self {
            schema_relevance: 0.30,
            temporal_coverage: 0.20,
            information_gain: 0.20,
            data_quality: 0.15,
            freshness: 0.10,
            dedup_value: 0.05,
        }
    }
}

pub struct FreeEvaluatorUDF {
    config: FreeEvaluatorUDFConfig,
}

impl FreeEvaluatorUDF {
    pub fn new(config: FreeEvaluatorUDFConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl ValuationUDF for FreeEvaluatorUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: LazyLock<UDFMetadata, fn() -> UDFMetadata> = LazyLock::new(|| {
            UDFMetadata {
            id: UDFId(crate::valuation::UDF_ID_FREE.into()),
            category: UDFCategory::Valuation,
            name: "Free Dataset Evaluator".into(),
            version: "1.0.0".into(),
            author: "Guixu Team".into(),
            description: "Evaluates free datasets by Task Fitness: schema relevance, temporal coverage, information gain, data quality, freshness, and deduplication value.".into(),
            tags: vec!["builtin".into(), "free".into(), "task-fitness".into()],
            parameters: vec![],
            capabilities: UFDCapabilities {
                task_types: vec!["*".into()],
                data_types: vec![],
            },
            limits: UDFLimits::default(),
        }
        });
        &METADATA
    }

    async fn evaluate(
        &self,
        input: &ValuationInput,
    ) -> Result<ValuationOutput, crate::common::UDFError> {
        let w = &self.config.weights;

        let schema_relevance = self.compute_schema_relevance(input);
        let temporal_coverage = self.compute_temporal_coverage(input);
        let information_gain = self.compute_information_gain(input);
        let data_quality = input
            .metadata
            .stats
            .as_ref()
            .map(|s| (1.0 - s.null_rate) * 100.0)
            .unwrap_or(50.0);
        let freshness = {
            let age = (chrono::Utc::now() - input.metadata.updated_at).num_days() as f64;
            (100.0 - age * 0.5).max(0.0)
        };
        let dedup_value = self.compute_dedup_value(input);

        let total = schema_relevance * w.schema_relevance
            + temporal_coverage * w.temporal_coverage
            + information_gain * w.information_gain
            + data_quality * w.data_quality
            + freshness * w.freshness
            + dedup_value * w.dedup_value;

        let breakdown = vec![
            crate::valuation::ScoreComponent::new(
                "schema_relevance",
                "Schema Relevance",
                schema_relevance,
                w.schema_relevance,
            ),
            crate::valuation::ScoreComponent::new(
                "temporal_coverage",
                "Temporal Coverage",
                temporal_coverage,
                w.temporal_coverage,
            ),
            crate::valuation::ScoreComponent::new(
                "information_gain",
                "Information Gain",
                information_gain,
                w.information_gain,
            ),
            crate::valuation::ScoreComponent::new(
                "data_quality",
                "Data Quality",
                data_quality,
                w.data_quality,
            ),
            crate::valuation::ScoreComponent::new("freshness", "Freshness", freshness, w.freshness),
            crate::valuation::ScoreComponent::new(
                "dedup_value",
                "Dedup Value",
                dedup_value,
                w.dedup_value,
            ),
        ];

        Ok(ValuationOutput {
            score: total,
            verdict: ValuationVerdict::from_score(total),
            breakdown,
            explanation: format!("Free evaluator for dataset {}", input.cid_str()),
            metadata: serde_json::json!({}),
        })
    }
}

impl FreeEvaluatorUDF {
    fn compute_schema_relevance(&self, input: &ValuationInput) -> f64 {
        if input.required_columns.is_empty() {
            return 50.0;
        }
        let dataset_cols: Vec<String> = input
            .metadata
            .schema
            .columns
            .iter()
            .map(|c| c.name.to_lowercase())
            .collect();
        let matched = input
            .required_columns
            .iter()
            .filter(|rc| {
                dataset_cols
                    .iter()
                    .any(|dc| dc.contains(&rc.to_lowercase()))
            })
            .count();
        (matched as f64 / input.required_columns.len() as f64) * 100.0
    }

    fn compute_temporal_coverage(&self, input: &ValuationInput) -> f64 {
        if let Some((start, end)) = &input.time_range {
            let task_start = chrono::NaiveDate::parse_from_str(start, "%Y-%m-%d");
            let task_end = chrono::NaiveDate::parse_from_str(end, "%Y-%m-%d");
            match (task_start, task_end) {
                (Ok(ts), Ok(te)) => {
                    let dataset_start = input.metadata.created_at.date_naive();
                    let dataset_end = input.metadata.updated_at.date_naive();
                    let overlap_start = ts.max(dataset_start);
                    let overlap_end = te.min(dataset_end);
                    if overlap_end > overlap_start {
                        let overlap_days = (overlap_end - overlap_start).num_days() as f64;
                        let task_days = (te - ts).num_days().max(1) as f64;
                        (overlap_days / task_days * 100.0).clamp(0.0, 100.0)
                    } else {
                        0.0
                    }
                }
                _ => 50.0,
            }
        } else {
            50.0
        }
    }

    fn compute_information_gain(&self, input: &ValuationInput) -> f64 {
        if input.required_columns.is_empty() || input.metadata.schema.columns.is_empty() {
            return 50.0;
        }
        let dataset_cols: Vec<String> = input
            .metadata
            .schema
            .columns
            .iter()
            .map(|c| c.name.to_lowercase())
            .collect();
        let n_d = dataset_cols.len() as f64;
        let n_t = input.required_columns.len() as f64;
        let n_total = n_d + n_t;
        let mut pmi_sum: f64 = 0.0;
        let mut matches: f64 = 0.0;
        for req in &input.required_columns {
            let req_lower = req.to_lowercase();
            let best = dataset_cols
                .iter()
                .map(|dc| {
                    if dc == &req_lower {
                        1.0
                    } else if dc.contains(&req_lower) || req_lower.contains(dc) {
                        0.5
                    } else {
                        0.0
                    }
                })
                .fold(0.0_f64, f64::max);
            if best > 0.0 {
                matches += best;
                let p_joint = best / n_total;
                let p_d = 1.0 / n_d;
                let p_t = 1.0 / n_t;
                pmi_sum += (p_joint / (p_d * p_t)).ln().max(0.0);
            }
        }
        let coverage = matches / n_t;
        let pmi_norm = if pmi_sum > 0.0 {
            (pmi_sum / n_t).min(1.0)
        } else {
            0.0
        };
        (coverage * 60.0 + pmi_norm * 40.0).clamp(0.0, 100.0)
    }

    fn compute_dedup_value(&self, input: &ValuationInput) -> f64 {
        if input.existing_data_cids.is_empty() {
            100.0
        } else {
            70.0
        }
    }
}

pub struct PaidEvaluatorUDFConfig {
    pub roi_weight: f64,
    pub quality_weight: f64,
}

impl Default for PaidEvaluatorUDFConfig {
    fn default() -> Self {
        Self {
            roi_weight: 0.6,
            quality_weight: 0.4,
        }
    }
}

pub struct PaidEvaluatorUDF {
    config: PaidEvaluatorUDFConfig,
}

impl PaidEvaluatorUDF {
    pub fn new(config: PaidEvaluatorUDFConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl ValuationUDF for PaidEvaluatorUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: LazyLock<UDFMetadata, fn() -> UDFMetadata> =
            LazyLock::new(|| UDFMetadata {
                id: UDFId(crate::valuation::UDF_ID_PAID.into()),
                category: UDFCategory::Valuation,
                name: "Paid Dataset Evaluator".into(),
                version: "1.0.0".into(),
                author: "Guixu Team".into(),
                description: "Evaluates paid datasets using ROI analysis and quality assessment."
                    .into(),
                tags: vec!["builtin".into(), "paid".into(), "roi".into()],
                parameters: vec![],
                capabilities: UFDCapabilities {
                    task_types: vec!["*".into()],
                    data_types: vec![],
                },
                limits: UDFLimits::default(),
            });
        &METADATA
    }

    async fn evaluate(
        &self,
        input: &ValuationInput,
    ) -> Result<ValuationOutput, crate::common::UDFError> {
        let roi = self.compute_roi(input);
        let quality = input
            .metadata
            .stats
            .as_ref()
            .map(|s| (1.0 - s.null_rate) * 100.0)
            .unwrap_or(50.0);

        let score = self.config.roi_weight * roi + self.config.quality_weight * quality;

        let breakdown = vec![
            crate::valuation::ScoreComponent::new("roi", "ROI Score", roi, self.config.roi_weight),
            crate::valuation::ScoreComponent::new(
                "quality",
                "Quality Score",
                quality,
                self.config.quality_weight,
            ),
        ];

        Ok(ValuationOutput {
            score,
            verdict: ValuationVerdict::from_score(score),
            breakdown,
            explanation: format!("Paid evaluator for dataset {}", input.cid_str()),
            metadata: serde_json::json!({
                "price_amount": input.metadata.price.amount,
                "price_currency": input.metadata.price.currency,
            }),
        })
    }
}

impl PaidEvaluatorUDF {
    fn compute_roi(&self, input: &ValuationInput) -> f64 {
        let price = input.metadata.price.amount;
        if price <= 0.0 {
            return 80.0;
        }
        let estimated_value = 100.0;
        let roi = (estimated_value - price) / price * 100.0;
        roi.clamp(0.0, 100.0)
    }
}

pub fn create_builtin_valuation_udf(
    id: &str,
    config: serde_json::Value,
) -> Result<Box<dyn ValuationUDF>, String> {
    match id {
        crate::valuation::UDF_ID_TCV => Ok(Box::new(TcvUDF::from_json(config)?)),
        crate::valuation::UDF_ID_FREE => {
            Ok(Box::new(FreeEvaluatorUDF::new(FreeEvaluatorUDFConfig {
                weights: FreeEvaluatorWeights::default(),
            })))
        }
        crate::valuation::UDF_ID_PAID => Ok(Box::new(PaidEvaluatorUDF::new(
            PaidEvaluatorUDFConfig::default(),
        ))),
        crate::valuation::UDF_ID_NOOP => Ok(Box::new(crate::valuation::NoOpValuationUDF)),
        _ => Err(format!("unknown builtin valuation UDF: {}", id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::valuation::{UDF_ID_NOOP, UDF_ID_TCV};

    #[test]
    fn test_tcv_weights_validate() {
        assert!(TcvWeights::default().validate());
        let mut w = TcvWeights::default();
        w.schema_fit = 0.5;
        w.temporal_fit = 0.5;
        assert!(!w.validate());
    }

    #[test]
    fn test_tcv_udf_from_json() {
        let config = serde_json::json!({
            "weights": {
                "schema_fit": 0.30,
                "temporal_fit": 0.20
            },
            "include_community_signal": false
        });
        let udf = TcvUDF::from_json(config).unwrap();
        assert_eq!(udf.config.weights.schema_fit, 0.30);
        assert!(!udf.config.include_community_signal);
    }

    #[test]
    fn test_create_builtin_valuation_udf() {
        let udf = create_builtin_valuation_udf(UDF_ID_TCV, serde_json::json!({})).unwrap();
        assert_eq!(udf.metadata().id.as_str(), UDF_ID_TCV);

        let udf = create_builtin_valuation_udf(UDF_ID_NOOP, serde_json::json!({})).unwrap();
        assert_eq!(udf.metadata().id.as_str(), UDF_ID_NOOP);

        let result =
            create_builtin_valuation_udf("builtin:valuation:unknown", serde_json::json!({}));
        assert!(result.is_err());
    }
}
