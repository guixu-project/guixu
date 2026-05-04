// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ValuationManifestResponse {
    pub dataset_id: String,
    pub commitment: Option<serde_json::Value>,
    pub manifest: Option<serde_json::Value>,
    pub supported_modes: Vec<String>,
    pub proof_system: String,
    pub request_url: String,
}

#[derive(Debug, Deserialize)]
pub struct ValuationReportResponse {
    pub id: Option<String>,
    pub request_id: Option<String>,
    pub score: Option<f64>,
    pub score_components: Option<serde_json::Value>,
    pub recommendation: Option<String>,
    pub risk_flags: Option<serde_json::Value>,
    pub proof_digest: Option<String>,
    pub proof_verdict: Option<String>,
    pub status: String,
}

pub struct ConfidentialValuationClient {
    http: Client,
    auth_headers: Vec<(String, String)>,
}

impl ConfidentialValuationClient {
    pub fn new(auth_headers: Vec<(String, String)>) -> Self {
        Self {
            http: Client::new(),
            auth_headers,
        }
    }

    /// Fetch the valuation manifest for a dataset.
    pub async fn get_manifest(&self, manifest_url: &str) -> Result<ValuationManifestResponse> {
        let mut req = self.http.get(manifest_url);
        for (k, v) in &self.auth_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        req.send()
            .await?
            .json()
            .await
            .context("Failed to parse manifest")
    }

    /// Request a confidential valuation.
    pub async fn request_valuation(
        &self,
        request_url: &str,
        dataset_id: &str,
        task_spec: serde_json::Value,
    ) -> Result<String> {
        let mut req = self.http.post(request_url)
            .json(&serde_json::json!({ "dataset_id": dataset_id, "task_spec": task_spec, "valuation_mode": "zk_committed_summary" }));
        for (k, v) in &self.auth_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp: serde_json::Value = req.send().await?.json().await?;
        resp["request_id"]
            .as_str()
            .map(String::from)
            .context("Missing request_id")
    }

    /// Poll for valuation report.
    pub async fn get_report(
        &self,
        base_url: &str,
        request_id: &str,
    ) -> Result<ValuationReportResponse> {
        let url = format!("{}/api/v1/confidential-valuations/{}", base_url, request_id);
        let mut req = self.http.get(&url);
        for (k, v) in &self.auth_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        req.send()
            .await?
            .json()
            .await
            .context("Failed to parse report")
    }
}
