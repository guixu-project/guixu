// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;

use super::util::infer_data_type_from_title;
use super::ExternalAdapter;
use tracing::{debug, warn};

pub struct BitTorrentAdapter {
    client: reqwest::Client,
}

impl Default for BitTorrentAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(3))
                .timeout(Duration::from_secs(8))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

impl BitTorrentAdapter {
    /// Try a single HTTP GET, return parsed JSON or error.
    async fn try_get(&self, url: &str, query: &[(&str, &str)]) -> Result<serde_json::Value> {
        let resp = tokio::time::timeout(
            Duration::from_secs(8),
            self.client.get(url).query(query).send(),
        )
        .await
        .map_err(|_| anyhow!("timeout"))??
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
        Ok(resp)
    }

    /// apibay.org — returns JSON array directly.
    async fn search_apibay(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let resp = self
            .try_get("https://apibay.org/q.php", &[("q", query), ("cat", "0")])
            .await?;

        let items = resp
            .as_array()
            .ok_or_else(|| anyhow!("apibay: expected array"))?;

        // apibay returns [{"id":"0","name":"No results",...}] for no results
        if items.len() == 1 {
            if let Some(name) = items[0].get("name").and_then(|v| v.as_str()) {
                if name == "No results returned" {
                    return Ok(vec![]);
                }
            }
        }

        Ok(items
            .iter()
            .take(limit)
            .filter_map(Self::parse_apibay)
            .collect())
    }

    fn parse_apibay(item: &serde_json::Value) -> Option<SearchResult> {
        let hash = item.get("info_hash")?.as_str()?;
        let name = item.get("name")?.as_str()?;
        let size = item
            .get("size")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let seeders = item
            .get("seeders")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let added = item
            .get("added")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let created = chrono::DateTime::from_timestamp(added, 0).unwrap_or_else(chrono::Utc::now);

        Some(SearchResult {
            cid: DatasetCid(hash.to_string()),
            title: name.to_string(),
            description: Some(format!("{seeders} seeders")),
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
            provider: Did(format!("bt:{hash}")),
            source: DataSource::BitTorrent,
            market: None,
            data_type: infer_data_type_from_title(name),
            created_at: created,
            seller_endpoint: None,
            source_attributes: None,
                    provider_meta: None,
                    governance: None,
        })
    }

    /// bitsearch.to — returns { results: [...] }.
    async fn search_bitsearch(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let limit_str = limit.min(100).to_string();
        let resp = self
            .try_get(
                "https://bitsearch.to/api/v1/search",
                &[("q", query), ("limit", &limit_str), ("sort", "seeders")],
            )
            .await?;

        let items = resp
            .get("results")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("bitsearch: missing 'results'"))?;

        Ok(items
            .iter()
            .take(limit)
            .filter_map(Self::parse_bitsearch)
            .collect())
    }

    fn parse_bitsearch(item: &serde_json::Value) -> Option<SearchResult> {
        let hash = item.get("infohash")?.as_str()?;
        let title = item.get("title")?.as_str()?;
        let size = item.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
        let seeders = item.get("seeders").and_then(|v| v.as_u64()).unwrap_or(0);
        let created = item
            .get("createdAt")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        Some(SearchResult {
            cid: DatasetCid(hash.to_string()),
            title: title.to_string(),
            description: Some(format!("{seeders} seeders")),
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
            provider: Did(format!("bt:{hash}")),
            source: DataSource::BitTorrent,
            market: None,
            data_type: infer_data_type_from_title(title),
            created_at: created,
            seller_endpoint: None,
            source_attributes: None,
                    provider_meta: None,
                    governance: None,
        })
    }

    /// solidtorrents.to — returns { results: [...] }.
    async fn search_solidtorrents(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let resp = self
            .try_get(
                "https://solidtorrents.to/api/v1/search",
                &[("q", query), ("sort", "seeders")],
            )
            .await?;

        let items = resp
            .get("results")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("solidtorrents: missing 'results'"))?;

        Ok(items
            .iter()
            .take(limit)
            .filter_map(|item| {
                let hash = item.get("infohash").or_else(|| item.get("_id"))?.as_str()?;
                let title = item.get("title")?.as_str()?;
                let size = item.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                let seeders = item
                    .pointer("/swarm/seeders")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                Some(SearchResult {
                    cid: DatasetCid(hash.to_string()),
                    title: title.to_string(),
                    description: Some(format!("{seeders} seeders")),
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
                    provider: Did(format!("bt:{hash}")),
                    source: DataSource::BitTorrent,
                    market: None,
                    data_type: infer_data_type_from_title(title),
                    created_at: chrono::Utc::now(),
                    seller_endpoint: None,
                    source_attributes: None,
                    provider_meta: None,
                    governance: None,
                })
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for BitTorrentAdapter {
    fn name(&self) -> &str {
        "bittorrent"
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // Try sources in order; return first success.
        match self.search_apibay(query, limit).await {
            Ok(r) => {
                debug!(
                    adapter = "bittorrent",
                    source = "apibay",
                    count = r.len(),
                    "ok"
                );
                return Ok(r);
            }
            Err(e) => {
                warn!(adapter = "bittorrent", source = "apibay", error = %e, "failed, trying next")
            }
        }
        match self.search_bitsearch(query, limit).await {
            Ok(r) => {
                debug!(
                    adapter = "bittorrent",
                    source = "bitsearch",
                    count = r.len(),
                    "ok"
                );
                return Ok(r);
            }
            Err(e) => {
                warn!(adapter = "bittorrent", source = "bitsearch", error = %e, "failed, trying next")
            }
        }
        match self.search_solidtorrents(query, limit).await {
            Ok(r) => {
                debug!(
                    adapter = "bittorrent",
                    source = "solidtorrents",
                    count = r.len(),
                    "ok"
                );
                return Ok(r);
            }
            Err(e) => {
                warn!(adapter = "bittorrent", source = "solidtorrents", error = %e, "all sources failed")
            }
        }
        Err(anyhow!("all BitTorrent search sources failed"))
    }
}

// ---------------------------------------------------------------------------
// Database adapters (PostgreSQL, DuckDB)
// ---------------------------------------------------------------------------
