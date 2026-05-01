// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::sync::{Arc, LazyLock};

use crate::common::{
    UDFCategory, UDFDescriptor, UDFError, UDFId, UDFMetadata, UDFResult, UFDCapabilities, UDFLimits,
};
use crate::sampling::{SampleRecord, SamplingInput, SamplingOutput, SamplingUDF};
use crate::valuation::{ValuationInput, ValuationOutput, ValuationUDF};

pub struct RemoteValuationUDF {
    endpoint: String,
    auth_token: Option<String>,
    client: reqwest::Client,
}

impl RemoteValuationUDF {
    pub fn new(endpoint: String, auth_token: Option<String>) -> Self {
        Self {
            endpoint,
            auth_token,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ValuationUDF for RemoteValuationUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: LazyLock<UDFMetadata, fn() -> UDFMetadata> = LazyLock::new(|| UDFMetadata {
            id: UDFId("builtin:valuation:remote".into()),
            category: UDFCategory::Valuation,
            name: "Remote Valuation UDF".into(),
            version: "1.0.0".into(),
            author: "Remote Service".into(),
            description: "Delegates valuation to a remote UDF service.".into(),
            tags: vec!["builtin".into(), "remote".into()],
            parameters: vec![],
            capabilities: UFDCapabilities::default(),
            limits: UDFLimits {
                max_execution_time_secs: 60,
                ..Default::default()
            },
        });
        &METADATA
    }

    async fn evaluate(&self, input: &ValuationInput) -> UDFResult<ValuationOutput> {
        let mut request = self.client.post(&self.endpoint);
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", token);
        }
        let response = request
            .json(input)
            .send()
            .await
            .map_err(|e| UDFError::RemoteError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(UDFError::RemoteError(format!(
                "Remote service returned status: {}",
                response.status()
            )));
        }

        response
            .json::<ValuationOutput>()
            .await
            .map_err(|e| UDFError::RemoteError(e.to_string()))
    }

    fn is_deterministic(&self) -> bool {
        false
    }
}

pub struct RemoteSamplingUDF {
    endpoint: String,
    auth_token: Option<String>,
    client: reqwest::Client,
}

impl RemoteSamplingUDF {
    pub fn new(endpoint: String, auth_token: Option<String>) -> Self {
        Self {
            endpoint,
            auth_token,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl SamplingUDF for RemoteSamplingUDF {
    fn metadata(&self) -> &UDFMetadata {
        static METADATA: LazyLock<UDFMetadata, fn() -> UDFMetadata> = LazyLock::new(|| UDFMetadata {
            id: UDFId("builtin:sampling:remote".into()),
            category: UDFCategory::Sampling,
            name: "Remote Sampling UDF".into(),
            version: "1.0.0".into(),
            author: "Remote Service".into(),
            description: "Delegates sampling to a remote UDF service.".into(),
            tags: vec!["builtin".into(), "remote".into()],
            parameters: vec![],
            capabilities: UFDCapabilities::default(),
            limits: UDFLimits {
                max_execution_time_secs: 60,
                ..Default::default()
            },
        });
        &METADATA
    }

    async fn sample(
        &self,
        input: &SamplingInput,
        all_records: &[SampleRecord],
    ) -> UDFResult<SamplingOutput> {
        let url = format!("{}/sample", self.endpoint);
        let mut request = self.client.post(&url);
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", token);
        }

        #[derive(serde::Serialize)]
        struct RemoteSamplingRequest<'a> {
            input: &'a SamplingInput,
            records: &'a [SampleRecord],
        }

        let response = request
            .json(&RemoteSamplingRequest {
                input,
                records: all_records,
            })
            .send()
            .await
            .map_err(|e| UDFError::RemoteError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(UDFError::RemoteError(format!(
                "Remote service returned status: {}",
                response.status()
            )));
        }

        response
            .json::<SamplingOutput>()
            .await
            .map_err(|e| UDFError::RemoteError(e.to_string()))
    }

    fn is_deterministic(&self) -> bool {
        false
    }
}

pub struct UDFServiceClient {
    base_url: String,
    auth_token: Option<String>,
    client: reqwest::Client,
}

impl UDFServiceClient {
    pub fn new(base_url: String, auth_token: Option<String>) -> Self {
        Self {
            base_url,
            auth_token,
            client: reqwest::Client::new(),
        }
    }

    pub async fn list_udfs(&self) -> UDFResult<Vec<UDFDescriptor>> {
        let mut request = self.client.get(&format!("{}/udfs", self.base_url));
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", token);
        }

        let response = request
            .send()
            .await
            .map_err(|e| UDFError::RemoteError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(UDFError::RemoteError(format!(
                "Failed to list UDFs: {}",
                response.status()
            )));
        }

        response
            .json::<Vec<UDFDescriptor>>()
            .await
            .map_err(|e| UDFError::RemoteError(e.to_string()))
    }

    pub async fn invoke_valuation(
        &self,
        udf_id: &UDFId,
        input: &ValuationInput,
    ) -> UDFResult<ValuationOutput> {
        let mut request = self.client.post(&format!("{}/valuation/{}", self.base_url, udf_id));
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", token);
        }

        let response = request
            .json(input)
            .send()
            .await
            .map_err(|e| UDFError::RemoteError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(UDFError::RemoteError(format!(
                "Remote valuation failed: {}",
                response.status()
            )));
        }

        response
            .json::<ValuationOutput>()
            .await
            .map_err(|e| UDFError::RemoteError(e.to_string()))
    }

    pub async fn invoke_sampling(
        &self,
        udf_id: &UDFId,
        input: &SamplingInput,
        records: &[SampleRecord],
    ) -> UDFResult<SamplingOutput> {
        let mut request = self.client.post(&format!("{}/sampling/{}", self.base_url, udf_id));
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", token);
        }

        #[derive(serde::Serialize)]
        struct Request<'a> {
            input: &'a SamplingInput,
            records: &'a [SampleRecord],
        }

        let response = request
            .json(&Request {
                input,
                records,
            })
            .send()
            .await
            .map_err(|e| UDFError::RemoteError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(UDFError::RemoteError(format!(
                "Remote sampling failed: {}",
                response.status()
            )));
        }

        response
            .json::<SamplingOutput>()
            .await
            .map_err(|e| UDFError::RemoteError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_valuation_udf_metadata() {
        let udf = RemoteValuationUDF::new(
            "http://localhost:8080/valuation".to_string(),
            Some("token".to_string()),
        );
        let metadata = udf.metadata();
        assert_eq!(metadata.id.as_str(), "builtin:valuation:remote");
        assert!(!udf.is_deterministic());
    }

    #[test]
    fn test_remote_sampling_udf_metadata() {
        let udf = RemoteSamplingUDF::new(
            "http://localhost:8080/sampling".to_string(),
            None,
        );
        let metadata = udf.metadata();
        assert_eq!(metadata.id.as_str(), "builtin:sampling:remote");
    }
}