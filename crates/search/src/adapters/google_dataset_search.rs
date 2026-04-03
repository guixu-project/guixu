// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;

use super::util::infer_data_type_from_title;
use super::ExternalAdapter;

pub struct GoogleDatasetSearchAdapter {
    client: reqwest::Client,
    api_key: Option<String>,
    cse_id: Option<String>,
    proxy_url: String,
}

impl Default for GoogleDatasetSearchAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_key: std::env::var("GOOGLE_API_KEY").ok(),
            cse_id: std::env::var("GOOGLE_CSE_ID").ok(),
            proxy_url: std::env::var("GUIXU_GOOGLE_SEARCH_PROXY_URL")
                .unwrap_or_else(|_| "https://www.guixu.org/api/search/google".into()),
        }
    }
}

impl GoogleDatasetSearchAdapter {
    /// Call Google CSE directly with user's own key.
    async fn search_direct(
        &self,
        query: &str,
        limit: usize,
        api_key: &str,
        cse_id: &str,
    ) -> Result<serde_json::Value> {
        let num = limit.min(10).to_string();
        let resp = tokio::time::timeout(
            Duration::from_secs(10),
            self.client
                .get("https://www.googleapis.com/customsearch/v1")
                .query(&[
                    ("key", api_key),
                    ("cx", cse_id),
                    ("q", query),
                    ("num", &num),
                ])
                .send(),
        )
        .await
        .map_err(|_| anyhow!("google dataset search: timeout"))??
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
        Ok(resp)
    }

    /// Fallback: call Guixu Hub proxy which uses Guixu's own key.
    async fn search_proxy(&self, query: &str, limit: usize) -> Result<serde_json::Value> {
        let num = limit.min(10).to_string();
        let resp = tokio::time::timeout(
            Duration::from_secs(10),
            self.client
                .get(&self.proxy_url)
                .query(&[("q", query), ("num", &num)])
                .send(),
        )
        .await
        .map_err(|_| anyhow!("google search proxy: timeout"))??
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
        Ok(resp)
    }

    fn parse_items(items: &[serde_json::Value], limit: usize) -> Vec<SearchResult> {
        items
            .iter()
            .take(limit)
            .filter_map(|item| {
                let title = item.get("title").and_then(|v| v.as_str())?;
                let link = item.get("link").and_then(|v| v.as_str()).unwrap_or("");
                let snippet = item
                    .get("snippet")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let cid_hash = {
                    use sha2::Digest;
                    let mut h = sha2::Sha256::new();
                    h.update(format!("gds:{title}:{link}").as_bytes());
                    hex::encode(h.finalize())
                };

                Some(SearchResult {
                    cid: DatasetCid(cid_hash),
                    title: title.to_string(),
                    description: snippet,
                    tags: vec![],
                    schema: DatasetSchema {
                        columns: vec![],
                        row_count: 0,
                        size_bytes: 0,
                    },
                    quality: None,
                    price: Price::free(),
                    license: License {
                        spdx_id: "unknown".into(),
                        commercial_use: false,
                        derivative_allowed: false,
                    },
                    provider: Did(format!("gds:{link}")),
                    source: DataSource::GoogleDatasetSearch,
                    market: None,
                    data_type: infer_data_type_from_title(title),
                    created_at: chrono::Utc::now(),
                    seller_endpoint: None,
                    source_attributes: None,
                })
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for GoogleDatasetSearchAdapter {
    fn name(&self) -> &str {
        "google_dataset_search"
    }
    fn source_type(&self) -> DataSource {
        DataSource::GoogleDatasetSearch
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let resp = match (&self.api_key, &self.cse_id) {
            (Some(k), Some(c)) => self.search_direct(query, limit, k, c).await?,
            _ => self.search_proxy(query, limit).await?,
        };

        let empty = vec![];
        let items = resp
            .get("items")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        Ok(Self::parse_items(items, limit))
    }
}

// ---------------------------------------------------------------------------
// DataCite Commons — uses the public DataCite REST API
// ---------------------------------------------------------------------------
