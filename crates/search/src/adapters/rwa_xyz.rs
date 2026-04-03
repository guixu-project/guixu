// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Result;
use data_core::types::*;

use super::ExternalAdapter;

const RWA_XYZ_TREASURIES_URL: &str = "https://api.rwa.xyz/v1/treasuries";

pub struct RwaXyzAdapter {
    client: reqwest::Client,
    api_key: Option<String>,
}

impl Default for RwaXyzAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            api_key: std::env::var("RWA_XYZ_API_KEY").ok(),
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for RwaXyzAdapter {
    fn name(&self) -> &str {
        "rwa_xyz"
    }
    fn source_type(&self) -> DataSource {
        DataSource::RwaXyz
    }
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let q = query.to_lowercase();
        let mut results = self.search_treasuries(&q, limit).await.unwrap_or_default();
        results.truncate(limit);
        Ok(results)
    }
}

impl RwaXyzAdapter {
    async fn search_treasuries(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let mut req = self.client.get(RWA_XYZ_TREASURIES_URL);
        if let Some(ref key) = self.api_key {
            req = req.header("x-api-key", key);
        }

        let items: Vec<serde_json::Value> = req.send().await?.error_for_status()?.json().await?;

        Ok(items
            .iter()
            .filter(|item| {
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let issuer = item.get("issuer").and_then(|v| v.as_str()).unwrap_or("");
                let symbol = item
                    .get("token_symbol")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                name.to_lowercase().contains(query)
                    || issuer.to_lowercase().contains(query)
                    || symbol.to_lowercase().contains(query)
                    || query.contains("rwa")
                    || query.contains("treasury")
            })
            .take(limit)
            .filter_map(|item| self.treasury_to_result(item))
            .collect())
    }

    fn treasury_to_result(&self, item: &serde_json::Value) -> Option<SearchResult> {
        let name = item.get("name")?.as_str()?;
        let issuer = item
            .get("issuer")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let chain = item
            .get("chain")
            .and_then(|v| v.as_str())
            .unwrap_or("ethereum");
        let symbol = item
            .get("token_symbol")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let tvl = item.get("tvl").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let apy = item.get("apy").and_then(|v| v.as_f64());
        let slug = name.to_lowercase().replace(' ', "-");

        Some(SearchResult {
            cid: DatasetCid(format!("rwa_xyz:treasury:{slug}")),
            title: format!("{name} — Tokenized Treasury by {issuer}"),
            description: Some(format!(
                "Tokenized treasury product by {issuer} on {chain}. TVL: ${:.0}M.{}",
                tvl / 1_000_000.0,
                apy.map(|a| format!(" APY: {a:.2}%.")).unwrap_or_default()
            )),
            tags: vec![
                "rwa".into(),
                "treasury".into(),
                "tokenized".into(),
                issuer.to_lowercase(),
                chain.to_lowercase(),
                "free".into(),
            ],
            schema: DatasetSchema {
                columns: vec![],
                row_count: 0,
                size_bytes: 0,
            },
            quality: None,
            price: Price::free(),
            license: License {
                spdx_id: "open-data".into(),
                commercial_use: true,
                derivative_allowed: true,
            },
            provider: Did(format!("source:rwa_xyz:{}", issuer.to_lowercase())),
            source: DataSource::RwaXyz,
            market: None,
            data_type: DataType::Tabular,
            created_at: chrono::Utc::now(),
            seller_endpoint: None,
            source_attributes: Some(serde_json::json!({
                "chain": chain.to_lowercase(),
                "protocol": issuer.to_lowercase(),
                "token_symbols": if symbol.is_empty() { vec![] } else { vec![symbol] },
                "issuer": issuer,
                "category": "rwa",
                "tvl_usd": tvl,
                "apy": apy,
                "refresh_cadence": "daily",
                "origin_url": format!("https://app.rwa.xyz/treasuries/{slug}"),
                "is_open_data": true,
            })),
        })
    }

    /// Pull full treasury catalog (for periodic sync).
    pub async fn fetch_full_treasury_catalog(&self) -> Result<Vec<SearchResult>> {
        self.search_treasuries("", usize::MAX).await
    }
}
