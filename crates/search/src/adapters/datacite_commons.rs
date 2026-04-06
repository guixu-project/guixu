// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;

use super::util::infer_data_type_from_title;
use super::ExternalAdapter;

pub struct DataCiteCommonsAdapter {
    client: reqwest::Client,
}

impl Default for DataCiteCommonsAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for DataCiteCommonsAdapter {
    fn name(&self) -> &str {
        "datacite_commons"
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let page_size = limit.min(25).to_string();
        let resp = tokio::time::timeout(
            Duration::from_secs(10),
            self.client
                .get("https://api.datacite.org/dois")
                .query(&[
                    ("query", query),
                    ("resource-type-id", "dataset"),
                    ("page[size]", &page_size),
                ])
                .send(),
        )
        .await
        .map_err(|_| anyhow!("datacite: timeout"))??
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

        let empty = vec![];
        let items = resp
            .get("data")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        Ok(items
            .iter()
            .take(limit)
            .filter_map(|item| {
                let attrs = item.get("attributes")?;
                let doi = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let title = attrs
                    .get("titles")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .and_then(|t| t.get("title"))
                    .and_then(|v| v.as_str())?;
                let desc = attrs
                    .get("descriptions")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .and_then(|d| d.get("description"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let year = attrs
                    .get("publicationYear")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let created = chrono::DateTime::parse_from_rfc3339(
                    attrs
                        .get("registered")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                )
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

                Some(SearchResult {
                    cid: DatasetCid(doi.to_string()),
                    title: title.to_string(),
                    description: desc.map(|d| if year > 0 { format!("[{year}] {d}") } else { d }),
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
                    provider: Did(format!("doi:{doi}")),
                    source: DataSource::DataCiteCommons,
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
// Guixu Hub — Guixu's public dataset hub and market index
//   Uses the Hub REST API and surfaces tags, schema, and market signals.
//   GUIXU_HUB_API_URL can override the default endpoint.
// ---------------------------------------------------------------------------
