// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;

use super::util::infer_data_type_from_title;
use super::ExternalAdapter;

// Kaggle — requires KAGGLE_USERNAME + KAGGLE_KEY env vars
// ---------------------------------------------------------------------------

pub struct KaggleAdapter {
    client: reqwest::Client,
    pub api_base: String,
    enabled: bool,
    proxy_url: String,
}

impl Default for KaggleAdapter {
    fn default() -> Self {
        let enabled =
            std::env::var("KAGGLE_USERNAME").is_ok() && std::env::var("KAGGLE_KEY").is_ok();
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_base: "https://www.kaggle.com/api/v1".into(),
            enabled,
            proxy_url: std::env::var("GUIXU_KAGGLE_PROXY_URL")
                .unwrap_or_else(|_| "https://www.guixu.org/api/search/kaggle".into()),
        }
    }
}

impl KaggleAdapter {
    fn parse_items(items: &[serde_json::Value], limit: usize) -> Vec<SearchResult> {
        items
            .iter()
            .take(limit)
            .filter_map(|item| {
                let title = item.get("title")?.as_str()?;
                let slug = item.get("ref")?.as_str().unwrap_or("");
                let size = item.get("totalBytes").and_then(|v| v.as_u64()).unwrap_or(0);
                let desc = item
                    .get("subtitle")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let license_name = item
                    .get("licenseName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let created = item
                    .get("lastUpdated")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);

                Some(SearchResult {
                    cid: DatasetCid(format!("kaggle:{slug}")),
                    title: title.to_string(),
                    description: desc,
                    tags: vec![],
                    schema: DatasetSchema {
                        columns: vec![],
                        row_count: 0,
                        size_bytes: size,
                    },
                    quality: None,
                    price: Price::free(),
                    license: License {
                        spdx_id: license_name.into(),
                        commercial_use: false,
                        derivative_allowed: false,
                    },
                    provider: Did(format!("kaggle:{slug}")),
                    source: DataSource::Kaggle,
                    market: None,
                    data_type: infer_data_type_from_title(title),
                    created_at: created,
                    seller_endpoint: None,
                    source_attributes: None,
                    provider_meta: None,
                    governance: None,
                })
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for KaggleAdapter {
    fn name(&self) -> &str {
        "kaggle"
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let resp = if self.enabled {
            let username = std::env::var("KAGGLE_USERNAME").unwrap_or_default();
            let key = std::env::var("KAGGLE_KEY").unwrap_or_default();
            let url = format!("{}/datasets/list", self.api_base);
            tokio::time::timeout(
                Duration::from_secs(10),
                self.client
                    .get(&url)
                    .basic_auth(&username, Some(&key))
                    .query(&[("search", query), ("maxSize", &limit.min(20).to_string())])
                    .send(),
            )
            .await
            .map_err(|_| anyhow!("kaggle: timeout"))??
            .error_for_status()?
            .json::<Vec<serde_json::Value>>()
            .await?
        } else {
            tokio::time::timeout(
                Duration::from_secs(10),
                self.client
                    .get(&self.proxy_url)
                    .query(&[("q", query), ("limit", &limit.min(20).to_string())])
                    .send(),
            )
            .await
            .map_err(|_| anyhow!("kaggle proxy: timeout"))??
            .error_for_status()?
            .json::<Vec<serde_json::Value>>()
            .await?
        };

        Ok(Self::parse_items(&resp, limit))
    }
}

// ---------------------------------------------------------------------------
// HuggingFace — requires HF_TOKEN env var
