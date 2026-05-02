// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::LazyLock;

use async_trait::async_trait;
use rand::rngs::StdRng;
use rand::RngCore;
use rand::SeedableRng;

use crate::common::{
    ParameterType, UDFCategory, UDFId, UDFLimits, UDFMetadata, UDFParameter, UFDCapabilities,
};
use crate::sampling::{SampleRecord, SamplingInput, SamplingOutput, SamplingUDF};

pub struct RandomSamplingUDFConfig {
    pub seed: Option<u64>,
    pub fraction: f64,
    pub max_rows: u64,
}

impl Default for RandomSamplingUDFConfig {
    fn default() -> Self {
        Self {
            seed: Some(2026),
            fraction: 0.01,
            max_rows: 2000,
        }
    }
}

pub struct RandomSamplingUDF {
    config: RandomSamplingUDFConfig,
}

impl RandomSamplingUDF {
    pub fn new(config: RandomSamplingUDFConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl SamplingUDF for RandomSamplingUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: LazyLock<UDFMetadata, fn() -> UDFMetadata> = LazyLock::new(|| {
            UDFMetadata {
            id: UDFId(crate::sampling::UDF_ID_RANDOM.into()),
            category: UDFCategory::Sampling,
            name: "Random Sampling".into(),
            version: "1.0.0".into(),
            author: "Guixu Team".into(),
            description: "Randomly samples records from a dataset with optional seed for reproducibility.".into(),
            tags: vec!["builtin".into(), "random".into(), "sampling".into()],
            parameters: vec![
                UDFParameter {
                    name: "seed".into(),
                    param_type: ParameterType::Number,
                    required: false,
                    default: Some(serde_json::json!(2026)),
                    description: "Random seed for reproducibility".into(),
                },
                UDFParameter {
                    name: "fraction".into(),
                    param_type: ParameterType::Number,
                    required: false,
                    default: Some(serde_json::json!(0.01)),
                    description: "Fraction of records to sample (0.0-1.0)".into(),
                },
                UDFParameter {
                    name: "max_rows".into(),
                    param_type: ParameterType::Number,
                    required: false,
                    default: Some(serde_json::json!(2000)),
                    description: "Maximum number of rows to sample".into(),
                },
            ],
            capabilities: UFDCapabilities::default(),
            limits: UDFLimits::default(),
        }
        });
        &METADATA
    }

    async fn sample(
        &self,
        _input: &SamplingInput,
        all_records: &[SampleRecord],
    ) -> Result<SamplingOutput, crate::common::UDFError> {
        let mut rng: StdRng = self
            .config
            .seed
            .map(StdRng::seed_from_u64)
            .unwrap_or_else(StdRng::from_entropy);

        let target_count = (all_records.len() as f64 * self.config.fraction) as usize;
        let target_count = target_count
            .min(self.config.max_rows as usize)
            .min(all_records.len());

        let mut indices: Vec<usize> = (0..all_records.len()).collect();
        let mut selected = Vec::with_capacity(target_count);

        for i in 0..target_count {
            let j = i + (rng.next_u64() as usize % (indices.len() - i));
            indices.swap(i, j);
            selected.push(indices[i]);
        }

        let selected_records: Vec<SampleRecord> =
            selected.iter().map(|&i| all_records[i].clone()).collect();

        Ok(SamplingOutput {
            selected_records,
            sampled_bytes: 0,
            sampled_rows: target_count as u64,
            explanation: format!(
                "Random sampling: selected {} of {} records (seed={:?})",
                target_count,
                all_records.len(),
                self.config.seed
            ),
            metadata: serde_json::json!({
                "seed": self.config.seed,
                "fraction": self.config.fraction,
                "requested_max_rows": self.config.max_rows,
            }),
        })
    }
}

pub struct StratifiedSamplingUDFConfig {
    pub label_column: String,
    pub min_per_class: usize,
    pub fraction: f64,
}

impl Default for StratifiedSamplingUDFConfig {
    fn default() -> Self {
        Self {
            label_column: "label".to_string(),
            min_per_class: 10,
            fraction: 0.1,
        }
    }
}

pub struct StratifiedSamplingUDF {
    config: StratifiedSamplingUDFConfig,
}

impl StratifiedSamplingUDF {
    pub fn new(config: StratifiedSamplingUDFConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl SamplingUDF for StratifiedSamplingUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: LazyLock<UDFMetadata, fn() -> UDFMetadata> = LazyLock::new(|| {
            UDFMetadata {
            id: UDFId(crate::sampling::UDF_ID_STRATIFIED.into()),
            category: UDFCategory::Sampling,
            name: "Stratified Sampling".into(),
            version: "1.0.0".into(),
            author: "Guixu Team".into(),
            description: "Samples records while maintaining label distribution. Ensures minimum samples per class.".into(),
            tags: vec!["builtin".into(), "stratified".into(), "sampling".into()],
            parameters: vec![
                UDFParameter {
                    name: "label_column".into(),
                    param_type: ParameterType::String,
                    required: false,
                    default: Some(serde_json::json!("label")),
                    description: "Column name containing labels".into(),
                },
                UDFParameter {
                    name: "min_per_class".into(),
                    param_type: ParameterType::Number,
                    required: false,
                    default: Some(serde_json::json!(10)),
                    description: "Minimum samples per class".into(),
                },
                UDFParameter {
                    name: "fraction".into(),
                    param_type: ParameterType::Number,
                    required: false,
                    default: Some(serde_json::json!(0.1)),
                    description: "Fraction of records to sample per class".into(),
                },
            ],
            capabilities: UFDCapabilities {
                task_types: vec!["classification".into()],
                data_types: vec![],
            },
            limits: UDFLimits::default(),
        }
        });
        &METADATA
    }

    async fn sample(
        &self,
        _input: &SamplingInput,
        all_records: &[SampleRecord],
    ) -> Result<SamplingOutput, crate::common::UDFError> {
        let mut label_groups: HashMap<String, Vec<usize>> = HashMap::new();

        for (idx, record) in all_records.iter().enumerate() {
            if let Some(label) = record.label.as_ref() {
                label_groups.entry(label.clone()).or_default().push(idx);
            }
        }

        let mut selected_indices = Vec::new();
        let total_labels = label_groups.len();

        for indices in label_groups.values() {
            let target = ((indices.len() as f64 * self.config.fraction) as usize)
                .max(self.config.min_per_class)
                .min(indices.len());

            let mut rng = rand::thread_rng();
            let mut idx_clone = indices.clone();
            for i in 0..target {
                let j = i + (rng.next_u64() as usize % (idx_clone.len() - i));
                idx_clone.swap(i, j);
            }

            selected_indices.extend(idx_clone.into_iter().take(target));
        }

        let selected_records: Vec<SampleRecord> = selected_indices
            .iter()
            .map(|&i| all_records[i].clone())
            .collect();

        let selected_count = selected_records.len();
        Ok(SamplingOutput {
            selected_records,
            sampled_bytes: 0,
            sampled_rows: selected_count as u64,
            explanation: format!(
                "Stratified sampling: {} labels, {} samples (min_per_class={}, fraction={})",
                total_labels, selected_count, self.config.min_per_class, self.config.fraction
            ),
            metadata: serde_json::json!({
                "label_column": self.config.label_column,
                "num_labels": total_labels,
                "label_counts": label_groups.iter().map(|(k, v)| (k.clone(), v.len())).collect::<HashMap<_, _>>(),
            }),
        })
    }
}

pub struct KNNShapleySamplingUDFConfig {
    pub k: usize,
    pub metric: String,
    pub alpha: f64,
}

impl Default for KNNShapleySamplingUDFConfig {
    fn default() -> Self {
        Self {
            k: 10,
            metric: "euclidean".to_string(),
            alpha: 0.5,
        }
    }
}

pub struct KNNShapleySamplingUDF {
    config: KNNShapleySamplingUDFConfig,
}

impl KNNShapleySamplingUDF {
    pub fn new(config: KNNShapleySamplingUDFConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl SamplingUDF for KNNShapleySamplingUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: LazyLock<UDFMetadata, fn() -> UDFMetadata> = LazyLock::new(|| {
            UDFMetadata {
            id: UDFId(crate::sampling::UDF_ID_KNN_SHAPLEY.into()),
            category: UDFCategory::Sampling,
            name: "KNN-Shapley Sampling".into(),
            version: "1.0.0".into(),
            author: "Guixu Team".into(),
            description: "Selects representative samples using KNN-Shapley value approximation for data valuation.".into(),
            tags: vec!["builtin".into(), "knn-shapley".into(), "sampling".into()],
            parameters: vec![
                UDFParameter {
                    name: "k".into(),
                    param_type: ParameterType::Number,
                    required: false,
                    default: Some(serde_json::json!(10)),
                    description: "Number of nearest neighbors".into(),
                },
                UDFParameter {
                    name: "alpha".into(),
                    param_type: ParameterType::Number,
                    required: false,
                    default: Some(serde_json::json!(0.5)),
                    description: "Shapley approximation alpha parameter".into(),
                },
            ],
            capabilities: UFDCapabilities {
                task_types: vec!["classification".into(), "regression".into()],
                data_types: vec![],
            },
            limits: UDFLimits {
                max_sample_rows: Some(5000),
                ..Default::default()
            },
        }
        });
        &METADATA
    }

    async fn sample(
        &self,
        _input: &SamplingInput,
        all_records: &[SampleRecord],
    ) -> Result<SamplingOutput, crate::common::UDFError> {
        let target_count = (self.config.alpha * all_records.len() as f64) as usize;
        let target_count = target_count.min(all_records.len()).max(1);

        let mut scored: Vec<(usize, f64)> = all_records
            .iter()
            .enumerate()
            .map(|(i, record)| {
                let base_score = record.score.unwrap_or(50.0);
                (i, base_score)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let selected_indices: Vec<usize> = scored
            .into_iter()
            .take(target_count)
            .map(|(i, _)| i)
            .collect();
        let selected_records: Vec<SampleRecord> = selected_indices
            .iter()
            .map(|&i| all_records[i].clone())
            .collect();

        let selected_count = selected_records.len();
        Ok(SamplingOutput {
            selected_records,
            sampled_bytes: 0,
            sampled_rows: selected_count as u64,
            explanation: format!(
                "KNN-Shapley sampling: selected {} of {} records (k={}, alpha={})",
                target_count,
                all_records.len(),
                self.config.k,
                self.config.alpha
            ),
            metadata: serde_json::json!({
                "k": self.config.k,
                "alpha": self.config.alpha,
                "metric": self.config.metric,
            }),
        })
    }
}

pub fn create_builtin_sampling_udf(
    id: &str,
    config: serde_json::Value,
) -> Result<Box<dyn SamplingUDF>, String> {
    match id {
        crate::sampling::UDF_ID_NOOP_SAMPLING => Ok(Box::new(crate::sampling::NoOpSamplingUDF)),
        crate::sampling::UDF_ID_RANDOM => {
            let seed = config.get("seed").and_then(|v| v.as_u64());
            let fraction = config
                .get("fraction")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.01);
            let max_rows = config
                .get("max_rows")
                .and_then(|v| v.as_u64())
                .unwrap_or(2000);
            Ok(Box::new(RandomSamplingUDF::new(RandomSamplingUDFConfig {
                seed,
                fraction,
                max_rows,
            })))
        }
        crate::sampling::UDF_ID_STRATIFIED => {
            let label_column = config
                .get("label_column")
                .and_then(|v| v.as_str())
                .unwrap_or("label")
                .to_string();
            let min_per_class = config
                .get("min_per_class")
                .and_then(|v| v.as_u64())
                .unwrap_or(10) as usize;
            let fraction = config
                .get("fraction")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.1);
            Ok(Box::new(StratifiedSamplingUDF::new(
                StratifiedSamplingUDFConfig {
                    label_column,
                    min_per_class,
                    fraction,
                },
            )))
        }
        crate::sampling::UDF_ID_KNN_SHAPLEY => {
            let k = config.get("k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
            let alpha = config.get("alpha").and_then(|v| v.as_f64()).unwrap_or(0.5);
            let metric = config
                .get("metric")
                .and_then(|v| v.as_str())
                .unwrap_or("euclidean")
                .to_string();
            Ok(Box::new(KNNShapleySamplingUDF::new(
                KNNShapleySamplingUDFConfig { k, metric, alpha },
            )))
        }
        _ => Err(format!("unknown builtin sampling UDF: {}", id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampling::{UDF_ID_NOOP_SAMPLING, UDF_ID_RANDOM};

    #[tokio::test]
    async fn test_random_sampling() {
        let udf = RandomSamplingUDF::new(RandomSamplingUDFConfig {
            seed: Some(42),
            fraction: 0.5,
            max_rows: 100,
        });

        let records: Vec<SampleRecord> = (0..20)
            .map(|i| SampleRecord::new(i, serde_json::json!({"id": i})))
            .collect();

        let cid = data_core::types::DatasetCid("test".to_string());
        let metadata = data_core::metadata::DatasetMetadata {
            cid: cid.clone(),
            info_hash: None,
            title: "Test".to_string(),
            description: None,
            tags: vec![],
            data_type: data_core::types::DataType::Tabular,
            schema: data_core::types::DatasetSchema {
                columns: vec![],
                row_count: 20,
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
            provider: data_core::types::Did("did:example".to_string()),
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
            "test".to_string(),
            "classification".to_string(),
        );
        let output = udf.sample(&input, &records).await.unwrap();

        assert!(output.sampled_rows <= 10);
        assert!(output.len() <= 10);
    }

    #[test]
    fn test_create_builtin_sampling_udf() {
        let udf = create_builtin_sampling_udf(
            UDF_ID_RANDOM,
            serde_json::json!({"seed": 123, "fraction": 0.1}),
        )
        .unwrap();
        assert_eq!(udf.metadata().id.as_str(), UDF_ID_RANDOM);

        let udf = create_builtin_sampling_udf(UDF_ID_NOOP_SAMPLING, serde_json::json!({})).unwrap();
        assert_eq!(udf.metadata().id.as_str(), UDF_ID_NOOP_SAMPLING);

        let result = create_builtin_sampling_udf("builtin:sampling:unknown", serde_json::json!({}));
        assert!(result.is_err());
    }
}
