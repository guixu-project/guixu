// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use data_core::types::DataType;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UDFCategory {
    Valuation,
    Sampling,
}

impl fmt::Display for UDFCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UDFCategory::Valuation => write!(f, "valuation"),
            UDFCategory::Sampling => write!(f, "sampling"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UFDCapabilities {
    #[serde(default)]
    pub task_types: Vec<String>,
    #[serde(default)]
    pub data_types: Vec<DataType>,
}

impl Default for UFDCapabilities {
    fn default() -> Self {
        Self {
            task_types: vec!["*".to_string()],
            data_types: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UDFLimits {
    #[serde(default = "default_max_memory")]
    pub max_memory_bytes: usize,
    #[serde(default = "default_max_execution_time")]
    pub max_execution_time_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_sample_rows: Option<u64>,
}

fn default_max_memory() -> usize {
    128 * 1024 * 1024
}

fn default_max_execution_time() -> u64 {
    30
}

impl Default for UDFLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: default_max_memory(),
            max_execution_time_secs: default_max_execution_time(),
            max_sample_rows: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UDFParameter {
    pub name: String,
    pub param_type: ParameterType,
    #[serde(default)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    pub description: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterType {
    String,
    Number,
    Boolean,
    Array,
    Object,
}

impl fmt::Display for ParameterType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParameterType::String => write!(f, "string"),
            ParameterType::Number => write!(f, "number"),
            ParameterType::Boolean => write!(f, "boolean"),
            ParameterType::Array => write!(f, "array"),
            ParameterType::Object => write!(f, "object"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UDFId(pub String);

impl UDFId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn category(&self) -> Option<UDFCategory> {
        if self.0.starts_with("builtin:valuation:") {
            Some(UDFCategory::Valuation)
        } else if self.0.starts_with("builtin:sampling:") {
            Some(UDFCategory::Sampling)
        } else {
            None
        }
    }

    pub fn namespace(&self) -> Option<&str> {
        self.0.split(':').next()
    }

    pub fn name(&self) -> Option<&str> {
        let parts: Vec<&str> = self.0.split(':').collect();
        if parts.len() > 2 {
            Some(&self.0[parts[0].len() + 1..])
        } else {
            parts.get(1).copied()
        }
    }

    pub fn is_builtin(&self) -> bool {
        self.0.starts_with("builtin:")
    }
}

impl fmt::Display for UDFId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for UDFId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for UDFId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for UDFId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UDFMetadata {
    pub id: UDFId,
    pub category: UDFCategory,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub parameters: Vec<UDFParameter>,
    #[serde(default)]
    pub capabilities: UFDCapabilities,
    #[serde(default)]
    pub limits: UDFLimits,
}

impl UDFMetadata {
    pub fn new(
        id: UDFId,
        category: UDFCategory,
        name: String,
        version: String,
        author: String,
        description: String,
    ) -> Self {
        Self {
            id,
            category,
            name,
            version,
            author,
            description,
            tags: vec![],
            parameters: vec![],
            capabilities: UFDCapabilities::default(),
            limits: UDFLimits::default(),
        }
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_parameters(mut self, parameters: Vec<UDFParameter>) -> Self {
        self.parameters = parameters;
        self
    }

    pub fn with_capabilities(mut self, capabilities: UFDCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_limits(mut self, limits: UDFLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn is_compatible_with_task(&self, task_type: &str) -> bool {
        if self.capabilities.task_types.is_empty() {
            return true;
        }
        self.capabilities.task_types.iter().any(|tt| {
            if tt == "*" {
                !task_type.contains('_')
            } else {
                tt == task_type
            }
        })
    }

    pub fn is_compatible_with_data_type(&self, data_type: DataType) -> bool {
        self.capabilities.data_types.is_empty() || self.capabilities.data_types.contains(&data_type)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UDFDescriptor {
    pub id: UDFId,
    pub category: UDFCategory,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub registered_at: DateTime<Utc>,
    pub enabled: bool,
}

impl From<&UDFMetadata> for UDFDescriptor {
    fn from(metadata: &UDFMetadata) -> Self {
        Self {
            id: metadata.id.clone(),
            category: metadata.category,
            name: metadata.name.clone(),
            version: metadata.version.clone(),
            author: metadata.author.clone(),
            description: metadata.description.clone(),
            tags: metadata.tags.clone(),
            registered_at: Utc::now(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UDFListFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<UDFCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_only: Option<bool>,
}

impl Default for UDFListFilter {
    fn default() -> Self {
        Self {
            category: None,
            tags: None,
            task_type: None,
            author: None,
            enabled_only: Some(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_udf_id_parsing() {
        let id = UDFId::new("builtin:valuation:tcv");
        assert_eq!(id.category(), Some(UDFCategory::Valuation));
        assert!(id.is_builtin());
        assert_eq!(id.namespace(), Some("builtin"));
        assert_eq!(id.name(), Some("valuation:tcv"));

        let id2 = UDFId::new("custom:my-sampler");
        assert_eq!(id2.category(), None);
        assert!(!id2.is_builtin());
    }

    #[test]
    fn test_udf_metadata_compatibility() {
        let metadata = UDFMetadata::new(
            UDFId::new("test:eval"),
            UDFCategory::Valuation,
            "Test".to_string(),
            "1.0.0".to_string(),
            "Test Author".to_string(),
            "Test description".to_string(),
        )
        .with_capabilities(UFDCapabilities {
            task_types: vec!["classification".to_string(), "*".to_string()],
            data_types: vec![DataType::Tabular],
        });

        assert!(metadata.is_compatible_with_task("classification"));
        assert!(metadata.is_compatible_with_task("regression"));
        assert!(!metadata.is_compatible_with_task("video_classification"));
        assert!(metadata.is_compatible_with_data_type(DataType::Tabular));
        assert!(!metadata.is_compatible_with_data_type(DataType::Video));
    }
}
