// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use data_udf::common::{UDFCategory, UDFId, UDFListFilter, UDFMetadata};
use data_udf::registry::UDFRegistry;
use data_udf::sampling::{SampleRecord, SamplingInput, SamplingUDF};
use data_udf::valuation::{ValuationInput, ValuationUDF};

fn create_test_metadata() -> data_core::metadata::DatasetMetadata {
    let cid = data_core::types::DatasetCid("test_cid_123".to_string());
    data_core::metadata::DatasetMetadata {
        cid: cid.clone(),
        info_hash: None,
        title: "Test Dataset for UDF".to_string(),
        description: Some("A test dataset for unit testing".to_string()),
        tags: vec!["test".to_string(), "tabular".to_string()],
        data_type: data_core::types::DataType::Tabular,
        schema: data_core::types::DatasetSchema {
            columns: vec![
                data_core::types::ColumnSchema {
                    name: "id".to_string(),
                    data_type: data_core::types::ColumnType::Integer,
                    description: Some("Primary key".to_string()),
                },
                data_core::types::ColumnSchema {
                    name: "feature1".to_string(),
                    data_type: data_core::types::ColumnType::Float,
                    description: None,
                },
                data_core::types::ColumnSchema {
                    name: "feature2".to_string(),
                    data_type: data_core::types::ColumnType::Float,
                    description: None,
                },
                data_core::types::ColumnSchema {
                    name: "label".to_string(),
                    data_type: data_core::types::ColumnType::String,
                    description: Some("Class label".to_string()),
                },
            ],
            row_count: 1000,
            size_bytes: 1024 * 1024,
        },
        stats: Some(data_core::types::DatasetStats {
            null_rate: 0.05,
            unique_rate: 0.8,
            min_value: None,
            max_value: None,
        }),
        video_meta: None,
        access: data_core::types::AccessMode::Open,
        price: data_core::types::Price {
            amount: 0.0,
            currency: "USD".to_string(),
        },
        license: data_core::types::License {
            spdx_id: "MIT".to_string(),
            name: "MIT License".to_string(),
        },
        provider: data_core::identity::Did("did:example:test".to_string()),
        signature: String::new(),
        provenance: data_core::metadata::Provenance::Original,
        created_at: chrono::Utc::now() - chrono::Duration::days(30),
        updated_at: chrono::Utc::now() - chrono::Duration::days(7),
        verifiable_credential: None,
        source_attributes: None,
        version: Some("1.0.0".to_string()),
        previous_version: None,
    }
}

fn create_test_records(count: usize) -> Vec<SampleRecord> {
    (0..count)
        .map(|i| {
            let label = if i % 2 == 0 { "positive" } else { "negative" };
            SampleRecord::new(
                i,
                serde_json::json!({
                    "id": i,
                    "feature1": i as f64 * 0.1,
                    "feature2": (i as f64 * 0.2).sin(),
                    "label": label,
                }),
            )
            .with_label(label.to_string())
            .with_score(50.0 + (i % 50) as f64)
        })
        .collect()
}

pub struct TestValuationUDF {
    return_score: f64,
}

impl TestValuationUDF {
    pub fn new(return_score: f64) -> Self {
        Self { return_score }
    }
}

#[data_udf::async_trait]
impl ValuationUDF for TestValuationUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: UDFMetadata = UDFMetadata {
            id: UDFId("test:valuation".to_string()),
            category: UDFCategory::Valuation,
            name: "Test Valuation UDF".to_string(),
            version: "1.0.0".to_string(),
            author: "Test".to_string(),
            description: "A test valuation UDF".to_string(),
            tags: vec!["test".to_string()],
            parameters: vec![],
            capabilities: data_udf::common::UFDCapabilities::default(),
            limits: data_udf::common::UDFLimits::default(),
        };
        &METADATA
    }

    async fn evaluate(&self, _input: &ValuationInput) -> Result<data_udf::valuation::ValuationOutput, data_udf::common::UDFError> {
        Ok(data_udf::valuation::ValuationOutput::new(
            self.return_score,
            "Test evaluation",
        ))
    }
}

pub struct TestSamplingUDF {
    return_count: usize,
}

impl TestSamplingUDF {
    pub fn new(return_count: usize) -> Self {
        Self { return_count }
    }
}

#[data_udf::async_trait]
impl SamplingUDF for TestSamplingUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: UDFMetadata = UDFMetadata {
            id: UDFId("test:sampling".to_string()),
            category: UDFCategory::Sampling,
            name: "Test Sampling UDF".to_string(),
            version: "1.0.0".to_string(),
            author: "Test".to_string(),
            description: "A test sampling UDF".to_string(),
            tags: vec!["test".to_string()],
            parameters: vec![],
            capabilities: data_udf::common::UFDCapabilities::default(),
            limits: data_udf::common::UDFLimits::default(),
        };
        &METADATA
    }

    async fn sample(
        &self,
        _input: &SamplingInput,
        all_records: &[SampleRecord],
    ) -> Result<data_udf::sampling::SamplingOutput, data_udf::common::UDFError> {
        let selected: Vec<SampleRecord> = all_records.iter().take(self.return_count).cloned().collect();
        Ok(data_udf::sampling::SamplingOutput::new("Test sampling").with_records(selected))
    }
}

#[tokio::test]
async fn test_registry_register_and_retrieve_valuation() {
    let mut registry = UDFRegistry::default();

    let udf = Box::new(TestValuationUDF::new(75.0));
    let id = registry.register_valuation(udf, serde_json::json!({})).unwrap();

    assert_eq!(id.as_str(), "test:valuation");
    assert_eq!(registry.valuation_count(), 1);
    assert_eq!(registry.total_count(), 1);

    let retrieved = registry.get_valuation(&id);
    assert!(retrieved.is_some());
}

#[tokio::test]
async fn test_registry_register_and_retrieve_sampling() {
    let mut registry = UDFRegistry::default();

    let udf = Box::new(TestSamplingUDF::new(100));
    let id = registry.register_sampling(udf, serde_json::json!({})).unwrap();

    assert_eq!(id.as_str(), "test:sampling");
    assert_eq!(registry.sampling_count(), 1);

    let retrieved = registry.get_sampling(&id);
    assert!(retrieved.is_some());
}

#[tokio::test]
async fn test_registry_evaluate_valuation() {
    let mut registry = UDFRegistry::default();

    let udf = Box::new(TestValuationUDF::new(85.0));
    registry.register_valuation(udf, serde_json::json!({})).unwrap();

    let metadata = create_test_metadata();
    let input = ValuationInput::new(
        data_core::types::DatasetCid("eval_test".to_string()),
        metadata,
        "Test classification task".to_string(),
        "classification".to_string(),
    )
    .with_required_columns(vec!["feature1".to_string(), "feature2".to_string()]);

    let result = registry.evaluate(&UDFId::new("test:valuation"), &input).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().score, 85.0);
}

#[tokio::test]
async fn test_registry_sample_sampling() {
    let mut registry = UDFRegistry::default();

    let udf = Box::new(TestSamplingUDF::new(50));
    registry.register_sampling(udf, serde_json::json!({})).unwrap();

    let records = create_test_records(200);
    let metadata = create_test_metadata();
    let input = SamplingInput::new(
        data_core::types::DatasetCid("sample_test".to_string()),
        metadata,
        "Test sampling task".to_string(),
        "classification".to_string(),
    )
    .with_budget(1024 * 1024, 100);

    let result = registry.sample(&UDFId::new("test:sampling"), &input, &records).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 50);
}

#[tokio::test]
async fn test_registry_unregister() {
    let mut registry = UDFRegistry::default();

    let val_udf = Box::new(TestValuationUDF::new(50.0));
    let val_id = registry.register_valuation(val_udf, serde_json::json!({})).unwrap();

    let samp_udf = Box::new(TestSamplingUDF::new(10));
    let samp_id = registry.register_sampling(samp_udf, serde_json::json!({})).unwrap();

    assert_eq!(registry.total_count(), 2);

    registry.unregister(&val_id).unwrap();
    assert_eq!(registry.total_count(), 1);
    assert!(registry.get_valuation(&val_id).is_none());

    registry.unregister(&samp_id).unwrap();
    assert_eq!(registry.total_count(), 0);
}

#[tokio::test]
async fn test_registry_enable_disable() {
    let mut registry = UDFRegistry::default();

    let udf = Box::new(TestValuationUDF::new(90.0));
    let id = registry.register_valuation(udf, serde_json::json!({})).unwrap();

    let metadata = create_test_metadata();
    let input = ValuationInput::new(
        data_core::types::DatasetCid("enable_test".to_string()),
        metadata,
        "Test task".to_string(),
        "classification".to_string(),
    );

    let result = registry.evaluate(&id, &input).await;
    assert!(result.is_ok());

    registry.set_enabled(&id, false).unwrap();

    let result = registry.evaluate(&id, &input).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_registry_list_filter_category() {
    let mut registry = UDFRegistry::default();

    registry.register_valuation(Box::new(TestValuationUDF::new(50.0)), serde_json::json!({})).unwrap();
    registry.register_sampling(Box::new(TestSamplingUDF::new(10)), serde_json::json!({})).unwrap();

    let all = registry.list(None);
    assert_eq!(all.len(), 2);

    let only_val = registry.list(Some(UDFListFilter {
        category: Some(UDFCategory::Valuation),
        ..Default::default()
    }));
    assert_eq!(only_val.len(), 1);
    assert_eq!(only_val[0].category, UDFCategory::Valuation);

    let only_samp = registry.list(Some(UDFListFilter {
        category: Some(UDFCategory::Sampling),
        ..Default::default()
    }));
    assert_eq!(only_samp.len(), 1);
    assert_eq!(only_samp[0].category, UDFCategory::Sampling);
}

#[tokio::test]
async fn test_registry_update_config() {
    let mut registry = UDFRegistry::default();

    let udf = Box::new(TestValuationUDF::new(50.0));
    let id = registry.register_valuation(udf, serde_json::json!({"key": "value"})).unwrap();

    let new_config = serde_json::json!({"key": "new_value", "extra": 123});
    registry.update_config(&id, new_config.clone()).unwrap();

    let descriptor = registry.list(None).into_iter().find(|d| d.id == id).unwrap();
    assert_eq!(descriptor.name, "Test Valuation UDF");
}

#[tokio::test]
async fn test_registry_not_found() {
    let registry = UDFRegistry::default();

    let metadata = create_test_metadata();
    let input = ValuationInput::new(
        data_core::types::DatasetCid("notfound".to_string()),
        metadata,
        "Test task".to_string(),
        "classification".to_string(),
    );

    let result = registry.evaluate(&UDFId::new("nonexistent:udf"), &input).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_tcv_udf_evaluation() {
    use data_udf::valuation::builtins::{create_builtin_valuation_udf, TcvUDF, TcvUDFConfig, TcvWeights};

    let udf = create_builtin_valuation_udf("builtin:valuation:tcv", serde_json::json!({})).unwrap();

    let metadata = create_test_metadata();
    let input = ValuationInput::new(
        data_core::types::DatasetCid("tcv_test".to_string()),
        metadata,
        "Classification task for tabular data".to_string(),
        "classification".to_string(),
    )
    .with_required_columns(vec!["feature1".to_string(), "feature2".to_string(), "label".to_string()]);

    let result = udf.evaluate(&input).await.unwrap();

    assert!(result.score >= 0.0 && result.score <= 100.0);
    assert!(result.breakdown.len() >= 4);
}

#[tokio::test]
async fn test_free_evaluator_udf() {
    use data_udf::valuation::builtins::create_builtin_valuation_udf;

    let udf = create_builtin_valuation_udf("builtin:valuation:free", serde_json::json!({})).unwrap();

    let metadata = create_test_metadata();
    let input = ValuationInput::new(
        data_core::types::DatasetCid("free_test".to_string()),
        metadata,
        "Free dataset evaluation".to_string(),
        "general".to_string(),
    )
    .with_required_columns(vec!["feature1".to_string()]);

    let result = udf.evaluate(&input).await.unwrap();

    assert!(result.score >= 0.0 && result.score <= 100.0);
}

#[tokio::test]
async fn test_paid_evaluator_udf() {
    use data_udf::valuation::builtins::create_builtin_valuation_udf;

    let udf = create_builtin_valuation_udf("builtin:valuation:paid", serde_json::json!({})).unwrap();

    let metadata = create_test_metadata();
    let input = ValuationInput::new(
        data_core::types::DatasetCid("paid_test".to_string()),
        metadata,
        "Paid dataset evaluation".to_string(),
        "regression".to_string(),
    );

    let result = udf.evaluate(&input).await.unwrap();

    assert!(result.score >= 0.0 && result.score <= 100.0);
}

#[tokio::test]
async fn test_random_sampling_udf() {
    use data_udf::sampling::builtins::create_builtin_sampling_udf;

    let udf = create_builtin_sampling_udf(
        "builtin:sampling:random",
        serde_json::json!({"seed": 42, "fraction": 0.1, "max_rows": 50}),
    ).unwrap();

    let records = create_test_records(500);
    let metadata = create_test_metadata();
    let input = SamplingInput::new(
        data_core::types::DatasetCid("random_test".to_string()),
        metadata,
        "Random sampling test".to_string(),
        "classification".to_string(),
    )
    .with_budget(1024 * 1024, 100);

    let result = udf.sample(&input, &records).await.unwrap();

    assert!(result.len() <= 50);
    assert!(result.len() > 0);
}

#[tokio::test]
async fn test_stratified_sampling_udf() {
    use data_udf::sampling::builtins::create_builtin_sampling_udf;

    let udf = create_builtin_sampling_udf(
        "builtin:sampling:stratified",
        serde_json::json!({"label_column": "label", "min_per_class": 5, "fraction": 0.2}),
    ).unwrap();

    let records = create_test_records(200);
    let metadata = create_test_metadata();
    let input = SamplingInput::new(
        data_core::types::DatasetCid("stratified_test".to_string()),
        metadata,
        "Stratified sampling test".to_string(),
        "classification".to_string(),
    );

    let result = udf.sample(&input, &records).await.unwrap();

    assert!(result.len() > 0);
}

#[tokio::test]
async fn test_knn_shapley_sampling_udf() {
    use data_udf::sampling::builtins::create_builtin_sampling_udf;

    let udf = create_builtin_sampling_udf(
        "builtin:sampling:knn_shapley",
        serde_json::json!({"k": 5, "alpha": 0.3}),
    ).unwrap();

    let records = create_test_records(100);
    let metadata = create_test_metadata();
    let input = SamplingInput::new(
        data_core::types::DatasetCid("knn_test".to_string()),
        metadata,
        "KNN-Shapley sampling test".to_string(),
        "classification".to_string(),
    );

    let result = udf.sample(&input, &records).await.unwrap();

    assert!(result.len() <= 30);
    assert!(result.len() > 0);
}

#[tokio::test]
async fn test_noop_valuation_udf() {
    use data_udf::valuation::NoOpValuationUDF;

    let udf = Box::new(NoOpValuationUDF);

    let metadata = create_test_metadata();
    let input = ValuationInput::new(
        data_core::types::DatasetCid("noop_test".to_string()),
        metadata,
        "No-op test".to_string(),
        "classification".to_string(),
    );

    let result = udf.evaluate(&input).await.unwrap();

    assert_eq!(result.score, 50.0);
    assert_eq!(result.verdict, data_udf::valuation::ValuationVerdict::Neutral);
}

#[tokio::test]
async fn test_noop_sampling_udf() {
    use data_udf::sampling::NoOpSamplingUDF;

    let udf = Box::new(NoOpSamplingUDF);

    let records = create_test_records(100);
    let metadata = create_test_metadata();
    let input = SamplingInput::new(
        data_core::types::DatasetCid("noop_samp_test".to_string()),
        metadata,
        "No-op sampling test".to_string(),
        "classification".to_string(),
    )
    .with_budget(u64::MAX, 20);

    let result = udf.sample(&input, &records).await.unwrap();

    assert_eq!(result.len(), 20);
}

#[tokio::test]
async fn test_udf_id_parsing() {
    let id = UDFId::new("builtin:valuation:tcv");
    assert_eq!(id.category(), Some(UDFCategory::Valuation));
    assert!(id.is_builtin());
    assert_eq!(id.namespace(), Some("builtin"));
    assert_eq!(id.name(), Some("valuation:tcv"));

    let id = UDFId::new("custom:my-sampler");
    assert_eq!(id.category(), None);
    assert!(!id.is_builtin());

    let id = UDFId::new("builtin:sampling:random");
    assert_eq!(id.category(), Some(UDFCategory::Sampling));
}

#[tokio::test]
async fn test_valuation_verdict_from_score() {
    use data_udf::valuation::ValuationVerdict;

    assert_eq!(ValuationVerdict::from_score(90.0), ValuationVerdict::StrongPositive);
    assert_eq!(ValuationVerdict::from_score(75.0), ValuationVerdict::Positive);
    assert_eq!(ValuationVerdict::from_score(55.0), ValuationVerdict::Neutral);
    assert_eq!(ValuationVerdict::from_score(40.0), ValuationVerdict::Negative);
    assert_eq!(ValuationVerdict::from_score(25.0), ValuationVerdict::StrongNegative);
}

#[tokio::test]
async fn test_valuation_input_builder() {
    let cid = data_core::types::DatasetCid("builder_test".to_string());
    let metadata = create_test_metadata();

    let input = ValuationInput::new(cid.clone(), metadata, "Test task".to_string(), "classification".to_string())
        .with_required_columns(vec!["col1".to_string(), "col2".to_string()])
        .with_time_range("2024-01-01", "2024-12-31")
        .with_budget(1000.0);

    assert_eq!(input.cid_str(), "builder_test");
    assert_eq!(input.required_columns.len(), 2);
    assert_eq!(input.time_range, Some(("2024-01-01".to_string(), "2024-12-31".to_string())));
    assert_eq!(input.budget, 1000.0);
}

#[tokio::test]
async fn test_sampling_requirements() {
    use data_udf::sampling::SampleRequirements;

    let reqs = SampleRequirements::new("Test requirements")
        .with_required_signals(vec!["label".to_string(), "feature".to_string()])
        .with_preferred_labels(vec!["positive".to_string(), "negative".to_string()]);

    assert_eq!(reqs.summary, "Test requirements");
    assert_eq!(reqs.required_signals.len(), 2);
    assert_eq!(reqs.preferred_labels.len(), 2);
}

#[tokio::test]
async fn test_sandbox_policy_builtin() {
    use data_udf::sandbox::SandboxPolicy;

    let policy = SandboxPolicy::default();

    assert!(policy.check_execution(&UDFId::new("builtin:valuation:tcv")).is_ok());
    assert!(policy.check_execution(&UDFId::new("builtin:sampling:random")).is_ok());
    assert!(policy.max_memory_bytes() == 128 * 1024 * 1024);
    assert!(policy.max_execution_time_secs() == 30);
}

#[tokio::test]
async fn test_remote_udf_metadata() {
    use data_udf::remote::{RemoteValuationUDF, RemoteSamplingUDF};

    let val_udf = RemoteValuationUDF::new(
        "http://localhost:8080/valuation".to_string(),
        Some("token123".to_string()),
    );
    let metadata = val_udf.metadata();
    assert_eq!(metadata.category, UDFCategory::Valuation);
    assert!(!val_udf.is_deterministic());

    let samp_udf = RemoteSamplingUDF::new(
        "http://localhost:8080/sampling".to_string(),
        None,
    );
    let metadata = samp_udf.metadata();
    assert_eq!(metadata.category, UDFCategory::Sampling);
}

#[test]
fn test_metadata_compatibility() {
    let metadata = UDFMetadata::new(
        UDFId::new("test:eval"),
        UDFCategory::Valuation,
        "Test".to_string(),
        "1.0.0".to_string(),
        "Test Author".to_string(),
        "Test description".to_string(),
    )
    .with_capabilities(data_udf::common::UFDCapabilities {
        task_types: vec!["classification".to_string(), "*".to_string()],
        data_types: vec![data_core::types::DataType::Tabular],
    });

    assert!(metadata.is_compatible_with_task("classification"));
    assert!(metadata.is_compatible_with_task("regression"));
    assert!(!metadata.is_compatible_with_task("video_classification"));
    assert!(metadata.is_compatible_with_data_type(data_core::types::DataType::Tabular));
    assert!(!metadata.is_compatible_with_data_type(data_core::types::DataType::Video));
}

#[tokio::test]
async fn test_registry_trait_object_safety() {
    use data_udf::valuation::ValuationUDF;
    use data_udf::sampling::SamplingUDF;

    let registry = UDFRegistry::default();

    fn _assert_udf<T: ValuationUDF + 'static>(_udf: &dyn ValuationUDF) {}
    fn _assert_sampling<T: SamplingUDF + 'static>(_udf: &dyn SamplingUDF) {}

    let _ = registry;
}

#[test]
fn test_chained_combination_modes() {
    use data_udf::chained::{ValuationCombinationMode, SamplingCombinationMode};

    assert_eq!(format!("{:?}", ValuationCombinationMode::WeightedSum), "WeightedSum");
    assert_eq!(format!("{:?}", ValuationCombinationMode::GeometricMean), "GeometricMean");
    assert_eq!(format!("{:?}", ValuationCombinationMode::Max), "Max");
    assert_eq!(format!("{:?}", ValuationCombinationMode::Min), "Min");
    assert_eq!(format!("{:?}", ValuationCombinationMode::RankAverage), "RankAverage");

    assert_eq!(format!("{:?}", SamplingCombinationMode::Concat), "Concat");
    assert_eq!(format!("{:?}", SamplingCombinationMode::Union), "Union");
    assert_eq!(format!("{:?}", SamplingCombinationMode::Intersect), "Intersect");
    assert_eq!(format!("{:?}", SamplingCombinationMode::FirstNonEmpty), "FirstNonEmpty");
}