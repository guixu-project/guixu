// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;

use super::util::infer_data_type_from_title;
use super::ExternalAdapter;

pub struct IpfsAdapter {
    client: reqwest::Client,
    pub gateway_url: String,
    search_url: String,
}

impl Default for IpfsAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            gateway_url: "https://ipfs.io".into(),
            search_url: std::env::var("IPFS_SEARCH_URL")
                .unwrap_or_else(|_| "https://api.ipfs-search.com/v1/search".into()),
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for IpfsAdapter {
    fn name(&self) -> &str {
        "ipfs"
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let page_size = limit.min(100).to_string();
        let resp = tokio::time::timeout(
            Duration::from_secs(10),
            self.client
                .get(&self.search_url)
                .query(&[("q", query), ("page_size", &page_size), ("type", "file")])
                .send(),
        )
        .await
        .map_err(|_| anyhow!("ipfs-search: timeout"))??
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

        let empty = vec![];
        let hits = resp
            .get("hits")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        Ok(hits
            .iter()
            .take(limit)
            .filter_map(|hit| {
                let hash = hit.get("hash")?.as_str()?;
                let title = hit.get("title").and_then(|v| v.as_str()).unwrap_or(hash);
                let size = hit.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                let desc = hit
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let created = hit
                    .get("first-seen")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);

                Some(SearchResult {
                    cid: DatasetCid(hash.to_string()),
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
                        spdx_id: "unknown".into(),
                        commercial_use: false,
                        derivative_allowed: false,
                    },
                    provider: Did(format!("ipfs:{hash}")),
                    source: DataSource::Ipfs,
                    market: None,
                    data_type: infer_data_type_from_title(title),
                    created_at: created,
                    seller_endpoint: None,
                    source_attributes: None,
                    provider_meta: None,
                    governance: None,
                })
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// BitTorrent — multi-source search with fallback
//   1. apibay.org  (The Pirate Bay public API — most reliable)
//   2. bitsearch.to
//   3. solidtorrents.to
