// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Guixu MCP client for calling dataset tools via HTTP JSON-RPC.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tracing::{debug, info};

/// Client for calling Guixu MCP server tools.
pub struct GuixuMcpClient {
    client: Client,
    base_url: String,
}

/// JSON-RPC request to MCP server.
#[derive(Serialize)]
struct McpRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Value,
}

/// JSON-RPC response from MCP server.
#[derive(Deserialize)]
struct McpResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: u64,
    result: Option<Value>,
    error: Option<McpError>,
}

/// JSON-RPC error.
#[derive(Deserialize)]
struct McpError {
    code: i64,
    message: String,
}

impl GuixuMcpClient {
    /// Create a new MCP client.
    pub fn new(base_url: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Call an MCP tool.
    pub async fn call_tool(&self, tool_name: &str, args: Value) -> Result<Value> {
        let request = McpRequest {
            jsonrpc: "2.0".into(),
            id: 1,
            method: "tools/call".into(),
            params: json!({
                "name": tool_name,
                "arguments": args,
            }),
        };

        let url = format!("{}/mcp", self.base_url);
        debug!("Calling MCP tool {} at {}", tool_name, url);

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send MCP request")?;

        let mcp_response: McpResponse = response
            .json()
            .await
            .context("Failed to parse MCP response")?;

        if let Some(error) = mcp_response.error {
            anyhow::bail!("MCP error {}: {}", error.code, error.message);
        }

        mcp_response.result.context("MCP response missing result")
    }

    /// Search for datasets.
    pub async fn dataset_search(
        &self,
        query: &str,
        task_type: Option<&str>,
        limit: Option<u64>,
    ) -> Result<Value> {
        let mut args = json!({
            "query": query,
        });

        if let Some(task_type) = task_type {
            args["task_type"] = json!(task_type);
        }
        if let Some(limit) = limit {
            args["limit"] = json!(limit);
        }

        info!("Searching datasets: {}", query);
        self.call_tool("dataset_search", args).await
    }

    /// Purchase a dataset.
    pub async fn dataset_purchase(&self, cid: &str, max_price: Option<f64>) -> Result<Value> {
        let mut args = json!({
            "cid": cid,
        });

        if let Some(max_price) = max_price {
            args["max_price"] = json!(max_price);
        }

        info!("Purchasing dataset: {}", cid);
        self.call_tool("dataset_purchase", args).await
    }

    /// Get dataset preview/sample.
    pub async fn dataset_preview(&self, cid: &str) -> Result<Value> {
        let url = format!("{}/api/datasets/{}/preview", self.base_url, cid);
        debug!("Getting dataset preview: {}", cid);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to get dataset preview")?;

        let preview: Value = response
            .json()
            .await
            .context("Failed to parse preview response")?;

        Ok(preview)
    }

    /// Get dataset stats.
    pub async fn dataset_stats(&self, cid: &str) -> Result<Value> {
        let url = format!("{}/api/datasets/{}/stats", self.base_url, cid);
        debug!("Getting dataset stats: {}", cid);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to get dataset stats")?;

        let stats: Value = response
            .json()
            .await
            .context("Failed to parse stats response")?;

        Ok(stats)
    }

    /// Download dataset to local file via MCP server.
    /// This calls the dataset_download tool which handles the actual download.
    pub async fn dataset_download(&self, cid: &str, output_path: &str) -> Result<()> {
        info!("Downloading dataset {} via MCP", cid);

        // Call dataset_download tool - MCP server handles the actual download
        let result = self
            .call_tool("dataset_download", json!({"cid": cid}))
            .await?;

        // Extract the downloaded file path from response
        let downloaded_path = result
            .get("path")
            .and_then(|v| v.as_str())
            .context("MCP response missing file path")?;

        // Copy to requested output path if different
        if downloaded_path != output_path {
            std::fs::copy(downloaded_path, output_path)
                .context("Failed to copy downloaded file")?;
            info!("Copied to {}", output_path);
        } else {
            info!("Downloaded to {}", output_path);
        }

        Ok(())
    }
}
