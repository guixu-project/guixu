// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::common::{
    UDFCategory, UDFDescriptor, UDFError, UDFId, UDFListFilter, UDFMetadata, UDFResult,
};
use crate::sampling::{SampleRecord, SamplingInput, SamplingOutput, SamplingUDF};
use crate::sandbox::SandboxPolicy;
use crate::valuation::{ValuationInput, ValuationOutput, ValuationUDF};

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
        input: &SamplingInput,
        records: &[SampleRecord],
    ) -> UDFResult<SamplingOutput>;
}

#[async_trait]
impl UDFRegistryTrait for UDFRegistry {
    async fn evaluate_valuation(
        &self,
        id: &UDFId,
        input: &ValuationInput,
    ) -> UDFResult<ValuationOutput> {
        self.evaluate(id, input).await
    }

    async fn sample_sampling(
        &self,
        id: &UDFId,
        input: &SamplingInput,
        records: &[SampleRecord],
    ) -> UDFResult<SamplingOutput> {
        self.sample(id, input, records).await
    }
}

struct RegisteredValuationUDF {
    udf: Box<dyn ValuationUDF>,
    config: serde_json::Value,
    enabled: bool,
    registered_at: DateTime<Utc>,
}

struct RegisteredSamplingUDF {
    udf: Box<dyn SamplingUDF>,
    config: serde_json::Value,
    enabled: bool,
    registered_at: DateTime<Utc>,
}

pub struct UDFRegistry {
    valuation_udfs: HashMap<UDFId, RegisteredValuationUDF>,
    sampling_udfs: HashMap<UDFId, RegisteredSamplingUDF>,
    sandbox_policy: SandboxPolicy,
}

impl UDFRegistry {
    pub fn new(sandbox_policy: SandboxPolicy) -> Self {
        Self {
            valuation_udfs: HashMap::new(),
            sampling_udfs: HashMap::new(),
            sandbox_policy,
        }
    }

    pub fn register_valuation(
        &mut self,
        udf: Box<dyn ValuationUDF>,
        config: serde_json::Value,
    ) -> UDFResult<UDFId> {
        let id = UDFId(udf.metadata().id.0.clone());
        if self.valuation_udfs.contains_key(&id) {
            return Err(UDFError::AlreadyRegistered(id.to_string()));
        }
        self.sandbox_policy.check_execution(&id)?;
        self.valuation_udfs.insert(
            id.clone(),
            RegisteredValuationUDF {
                udf,
                config,
                enabled: true,
                registered_at: Utc::now(),
            },
        );
        Ok(id)
    }

    pub fn register_sampling(
        &mut self,
        udf: Box<dyn SamplingUDF>,
        config: serde_json::Value,
    ) -> UDFResult<UDFId> {
        let id = UDFId(udf.metadata().id.0.clone());
        if self.sampling_udfs.contains_key(&id) {
            return Err(UDFError::AlreadyRegistered(id.to_string()));
        }
        self.sandbox_policy.check_execution(&id)?;
        self.sampling_udfs.insert(
            id.clone(),
            RegisteredSamplingUDF {
                udf,
                config,
                enabled: true,
                registered_at: Utc::now(),
            },
        );
        Ok(id)
    }

    pub fn unregister(&mut self, id: &UDFId) -> UDFResult<()> {
        if self.valuation_udfs.remove(id).is_some() {
            return Ok(());
        }
        if self.sampling_udfs.remove(id).is_some() {
            return Ok(());
        }
        Err(UDFError::NotFound(id.to_string()))
    }

    pub fn get_valuation(&self, id: &UDFId) -> Option<&dyn ValuationUDF> {
        self.valuation_udfs
            .get(id)
            .filter(|r| r.enabled)
            .map(|r| r.udf.as_ref())
    }

    pub fn get_sampling(&self, id: &UDFId) -> Option<&dyn SamplingUDF> {
        self.sampling_udfs
            .get(id)
            .filter(|r| r.enabled)
            .map(|r| r.udf.as_ref())
    }

    pub async fn evaluate(&self, id: &UDFId, input: &ValuationInput) -> UDFResult<ValuationOutput> {
        let udf = self
            .valuation_udfs
            .get(id)
            .ok_or_else(|| UDFError::NotFound(id.to_string()))?;
        if !udf.enabled {
            return Err(UDFError::ExecutionError(format!(
                "UDF '{}' is disabled",
                id
            )));
        }
        self.sandbox_policy.check_execution(id)?;
        udf.udf.evaluate(input).await
    }

    pub async fn sample(
        &self,
        id: &UDFId,
        input: &SamplingInput,
        records: &[crate::sampling::SampleRecord],
    ) -> UDFResult<SamplingOutput> {
        let udf = self
            .sampling_udfs
            .get(id)
            .ok_or_else(|| UDFError::NotFound(id.to_string()))?;
        if !udf.enabled {
            return Err(UDFError::ExecutionError(format!(
                "UDF '{}' is disabled",
                id
            )));
        }
        self.sandbox_policy.check_execution(id)?;
        udf.udf.sample(input, records).await
    }

    pub fn list(&self, filter: Option<UDFListFilter>) -> Vec<UDFDescriptor> {
        let filter = filter.unwrap_or_default();
        let mut results = Vec::new();

        if filter
            .category
            .map(|c| c == UDFCategory::Valuation)
            .unwrap_or(true)
        {
            for (id, udf) in &self.valuation_udfs {
                if self.matches_filter(&filter, id, &udf.registered_at, udf.enabled) {
                    results.push(UDFDescriptor {
                        id: id.clone(),
                        category: UDFCategory::Valuation,
                        name: udf.udf.metadata().name.clone(),
                        version: udf.udf.metadata().version.clone(),
                        author: udf.udf.metadata().author.clone(),
                        description: udf.udf.metadata().description.clone(),
                        tags: udf.udf.metadata().tags.clone(),
                        registered_at: udf.registered_at,
                        enabled: udf.enabled,
                    });
                }
            }
        }

        if filter
            .category
            .map(|c| c == UDFCategory::Sampling)
            .unwrap_or(true)
        {
            for (id, udf) in &self.sampling_udfs {
                if self.matches_filter(&filter, id, &udf.registered_at, udf.enabled) {
                    results.push(UDFDescriptor {
                        id: id.clone(),
                        category: UDFCategory::Sampling,
                        name: udf.udf.metadata().name.clone(),
                        version: udf.udf.metadata().version.clone(),
                        author: udf.udf.metadata().author.clone(),
                        description: udf.udf.metadata().description.clone(),
                        tags: udf.udf.metadata().tags.clone(),
                        registered_at: udf.registered_at,
                        enabled: udf.enabled,
                    });
                }
            }
        }

        results
    }

    fn matches_filter(
        &self,
        filter: &UDFListFilter,
        id: &UDFId,
        _registered_at: &DateTime<Utc>,
        enabled: bool,
    ) -> bool {
        if filter.enabled_only.unwrap_or(true) && !enabled {
            return false;
        }
        if let Some(ref tags) = filter.tags {
            let udf_tags = if let Some(udf) = self.valuation_udfs.get(id) {
                &udf.udf.metadata().tags
            } else if let Some(udf) = self.sampling_udfs.get(id) {
                &udf.udf.metadata().tags
            } else {
                return false;
            };
            if !tags.iter().any(|t| udf_tags.contains(t)) {
                return false;
            }
        }
        if let Some(ref author) = filter.author {
            let udf_author = if let Some(udf) = self.valuation_udfs.get(id) {
                udf.udf.metadata().author.clone()
            } else if let Some(udf) = self.sampling_udfs.get(id) {
                udf.udf.metadata().author.clone()
            } else {
                return false;
            };
            if udf_author != *author {
                return false;
            }
        }
        if let Some(ref task_type) = filter.task_type {
            if task_type != "*" {
                let supported = if let Some(udf) = self.valuation_udfs.get(id) {
                    udf.udf.supported_task_types()
                } else if let Some(udf) = self.sampling_udfs.get(id) {
                    udf.udf.supported_task_types()
                } else {
                    return false;
                };
                if !supported.contains(task_type) && !supported.contains(&"*".to_string()) {
                    return false;
                }
            }
        }
        true
    }

    pub fn update_config(&mut self, id: &UDFId, config: serde_json::Value) -> UDFResult<()> {
        if let Some(udf) = self.valuation_udfs.get_mut(id) {
            udf.config = config;
            return Ok(());
        }
        if let Some(udf) = self.sampling_udfs.get_mut(id) {
            udf.config = config;
            return Ok(());
        }
        Err(UDFError::NotFound(id.to_string()))
    }

    pub fn set_enabled(&mut self, id: &UDFId, enabled: bool) -> UDFResult<()> {
        if let Some(udf) = self.valuation_udfs.get_mut(id) {
            udf.enabled = enabled;
            return Ok(());
        }
        if let Some(udf) = self.sampling_udfs.get_mut(id) {
            udf.enabled = enabled;
            return Ok(());
        }
        Err(UDFError::NotFound(id.to_string()))
    }

    pub fn get_metadata(&self, id: &UDFId) -> Option<UDFMetadata> {
        if let Some(udf) = self.valuation_udfs.get(id) {
            return Some(udf.udf.metadata().clone());
        }
        if let Some(udf) = self.sampling_udfs.get(id) {
            return Some(udf.udf.metadata().clone());
        }
        None
    }

    pub fn valuation_count(&self) -> usize {
        self.valuation_udfs.len()
    }

    pub fn sampling_count(&self) -> usize {
        self.sampling_udfs.len()
    }

    pub fn total_count(&self) -> usize {
        self.valuation_udfs.len() + self.sampling_udfs.len()
    }

    pub fn enabled_valuation_count(&self) -> usize {
        self.valuation_udfs.values().filter(|u| u.enabled).count()
    }

    pub fn enabled_sampling_count(&self) -> usize {
        self.sampling_udfs.values().filter(|u| u.enabled).count()
    }
}

impl Default for UDFRegistry {
    fn default() -> Self {
        Self::new(SandboxPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampling::NoOpSamplingUDF;
    use crate::valuation::NoOpValuationUDF;

    #[tokio::test]
    async fn test_registry_registration() {
        let mut registry = UDFRegistry::default();

        let udf = Box::new(NoOpValuationUDF);
        let id = registry
            .register_valuation(udf, serde_json::json!({}))
            .unwrap();
        assert_eq!(id.as_str(), "builtin:valuation:noop");

        let retrieved = registry.get_valuation(&id);
        assert!(retrieved.is_some());

        assert_eq!(registry.valuation_count(), 1);
        assert_eq!(registry.total_count(), 1);
    }

    #[tokio::test]
    async fn test_registry_evaluation() {
        let mut registry = UDFRegistry::default();

        let udf = Box::new(NoOpValuationUDF);
        registry
            .register_valuation(udf, serde_json::json!({}))
            .unwrap();

        let cid = data_core::types::DatasetCid("test".to_string());
        let metadata = data_core::metadata::DatasetMetadata {
            cid,
            info_hash: None,
            title: "Test".to_string(),
            description: None,
            tags: vec![],
            data_type: data_core::types::DataType::Tabular,
            schema: data_core::types::DatasetSchema {
                columns: vec![],
                row_count: 100,
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

        let input = ValuationInput::new(
            data_core::types::DatasetCid("test".to_string()),
            metadata,
            "Test task".to_string(),
            "classification".to_string(),
        );

        let result = registry
            .evaluate(&UDFId::new("builtin:valuation:noop"), &input)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().score, 50.0);
    }

    #[tokio::test]
    async fn test_registry_unregister() {
        let mut registry = UDFRegistry::default();

        let udf = Box::new(NoOpValuationUDF);
        let id = registry
            .register_valuation(udf, serde_json::json!({}))
            .unwrap();

        registry.unregister(&id).unwrap();
        assert!(registry.get_valuation(&id).is_none());
        assert_eq!(registry.valuation_count(), 0);
    }

    #[tokio::test]
    async fn test_registry_enabled_toggle() {
        let mut registry = UDFRegistry::default();

        let udf = Box::new(NoOpValuationUDF);
        let id = registry
            .register_valuation(udf, serde_json::json!({}))
            .unwrap();

        registry.set_enabled(&id, false).unwrap();

        let cid = data_core::types::DatasetCid("test".to_string());
        let metadata = data_core::metadata::DatasetMetadata {
            cid,
            info_hash: None,
            title: "Test".to_string(),
            description: None,
            tags: vec![],
            data_type: data_core::types::DataType::Tabular,
            schema: data_core::types::DatasetSchema {
                columns: vec![],
                row_count: 100,
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

        let input = ValuationInput::new(
            data_core::types::DatasetCid("test".to_string()),
            metadata,
            "Test task".to_string(),
            "classification".to_string(),
        );

        let result = registry.evaluate(&id, &input).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_list_filter() {
        let mut registry = UDFRegistry::default();

        let udf = Box::new(NoOpValuationUDF);
        registry
            .register_valuation(udf, serde_json::json!({}))
            .unwrap();

        let sampling_udf = Box::new(NoOpSamplingUDF);
        registry
            .register_sampling(sampling_udf, serde_json::json!({}))
            .unwrap();

        let all = registry.list(None);
        assert_eq!(all.len(), 2);

        let only_valuation = registry.list(Some(UDFListFilter {
            category: Some(UDFCategory::Valuation),
            ..Default::default()
        }));
        assert_eq!(only_valuation.len(), 1);
        assert_eq!(only_valuation[0].category, UDFCategory::Valuation);

        let only_sampling = registry.list(Some(UDFListFilter {
            category: Some(UDFCategory::Sampling),
            ..Default::default()
        }));
        assert_eq!(only_sampling.len(), 1);
        assert_eq!(only_sampling[0].category, UDFCategory::Sampling);
    }

    #[tokio::test]
    async fn test_registry_not_found() {
        let registry = UDFRegistry::default();

        let cid = data_core::types::DatasetCid("test".to_string());
        let metadata = data_core::metadata::DatasetMetadata {
            cid,
            info_hash: None,
            title: "Test".to_string(),
            description: None,
            tags: vec![],
            data_type: data_core::types::DataType::Tabular,
            schema: data_core::types::DatasetSchema {
                columns: vec![],
                row_count: 100,
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

        let input = ValuationInput::new(
            data_core::types::DatasetCid("test".to_string()),
            metadata,
            "Test task".to_string(),
            "classification".to_string(),
        );

        let result = registry
            .evaluate(&UDFId::new("nonexistent:udf"), &input)
            .await;
        assert!(result.is_err());
    }
}
