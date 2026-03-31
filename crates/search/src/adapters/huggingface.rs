use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;

use super::util::infer_data_type_from_title;
use super::ExternalAdapter;

pub struct HuggingFaceAdapter {
    client: reqwest::Client,
    pub api_base: String,
    enabled: bool,
    proxy_url: String,
}

impl Default for HuggingFaceAdapter {
    fn default() -> Self {
        let enabled = std::env::var("HF_TOKEN").is_ok();
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_base: "https://huggingface.co/api".into(),
            enabled,
            proxy_url: std::env::var("GUIXU_HF_PROXY_URL")
                .unwrap_or_else(|_| "https://www.guixu.org/api/search/huggingface".into()),
        }
    }
}

impl HuggingFaceAdapter {
    fn parse_items(items: &[serde_json::Value], limit: usize) -> Vec<SearchResult> {
        items
            .iter()
            .take(limit)
            .filter_map(|item| {
                let id = item.get("id")?.as_str()?;
                let downloads = item.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0);
                let desc = item
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let created = item
                    .get("lastModified")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);
                let tags: Vec<String> = item
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|t| t.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                Some(SearchResult {
                    cid: DatasetCid(format!("hf:{id}")),
                    title: id.to_string(),
                    description: desc,
                    tags,
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
                    provider: Did(format!("hf:{id}")),
                    source: DataSource::HuggingFace,
                    market: Some(DatasetMarketStats {
                        download_count: downloads,
                        review_count: 0,
                        trade_count: 0,
                    }),
                    data_type: infer_data_type_from_title(id),
                    created_at: created,
                    seller_endpoint: None,
                    source_attributes: None,
                })
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for HuggingFaceAdapter {
    fn name(&self) -> &str {
        "huggingface"
    }
    fn source_type(&self) -> DataSource {
        DataSource::HuggingFace
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let resp = if self.enabled {
            let token = std::env::var("HF_TOKEN").unwrap_or_default();
            let url = format!("{}/datasets", self.api_base);
            tokio::time::timeout(
                Duration::from_secs(10),
                self.client
                    .get(&url)
                    .bearer_auth(&token)
                    .query(&[
                        ("search", query),
                        ("limit", &limit.min(100).to_string()),
                        ("sort", "downloads"),
                        ("direction", "-1"),
                    ])
                    .send(),
            )
            .await
            .map_err(|_| anyhow!("huggingface: timeout"))??
            .error_for_status()?
            .json::<Vec<serde_json::Value>>()
            .await?
        } else {
            tokio::time::timeout(
                Duration::from_secs(10),
                self.client
                    .get(&self.proxy_url)
                    .query(&[("q", query), ("limit", &limit.min(100).to_string())])
                    .send(),
            )
            .await
            .map_err(|_| anyhow!("huggingface proxy: timeout"))??
            .error_for_status()?
            .json::<Vec<serde_json::Value>>()
            .await?
        };

        Ok(Self::parse_items(&resp, limit))
    }
}

// ---------------------------------------------------------------------------
// IPFS
