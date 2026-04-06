// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Result;
use data_core::types::*;

use super::util::contains_any_adapter;
use super::ExternalAdapter;

// DefiLlama — free open DeFi data (stablecoins, bridges, protocols)
// ---------------------------------------------------------------------------

const DEFILLAMA_STABLECOINS_URL: &str =
    "https://stablecoins.llama.fi/stablecoins?includePrices=true";
const DEFILLAMA_BRIDGES_URL: &str = "https://api.llama.fi/v2/bridges";
const DEFILLAMA_PROTOCOLS_URL: &str = "https://api.llama.fi/protocols";

pub struct DefiLlamaAdapter {
    client: reqwest::Client,
}

impl Default for DefiLlamaAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for DefiLlamaAdapter {
    fn name(&self) -> &str {
        "defillama"
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let q = query.to_lowercase();
        let mut results = Vec::new();

        if contains_any_adapter(&q, &["stablecoin", "usdc", "usdt", "dai", "stable", "peg"]) {
            results.extend(self.search_stablecoins(&q, limit).await.unwrap_or_default());
        }
        if contains_any_adapter(&q, &["bridge", "cross-chain", "crosschain"]) {
            results.extend(self.search_bridges(&q, limit).await.unwrap_or_default());
        }
        if results.is_empty() {
            results.extend(self.search_protocols(&q, limit).await.unwrap_or_default());
        }

        results.truncate(limit);
        Ok(results)
    }
}

impl DefiLlamaAdapter {
    async fn search_stablecoins(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let resp: serde_json::Value = self
            .client
            .get(DEFILLAMA_STABLECOINS_URL)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let empty = vec![];
        let assets = resp
            .get("peggedAssets")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        Ok(assets
            .iter()
            .filter(|a| {
                let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let symbol = a.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                name.to_lowercase().contains(query)
                    || symbol.to_lowercase().contains(query)
                    || query.contains(&symbol.to_lowercase())
            })
            .take(limit)
            .filter_map(|asset| self.stablecoin_to_result(asset))
            .collect())
    }

    fn stablecoin_to_result(&self, asset: &serde_json::Value) -> Option<SearchResult> {
        let name = asset.get("name")?.as_str()?;
        let symbol = asset.get("symbol")?.as_str()?;
        let id = match asset.get("id")? {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            _ => return None,
        };
        let chains: Vec<String> = asset
            .get("chains")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let peg_type = asset
            .get("pegType")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        Some(SearchResult {
            cid: DatasetCid(format!("defillama:stablecoin:{}", symbol.to_lowercase())),
            title: format!("{name} ({symbol}) — Stablecoin Market Data"),
            description: Some(format!(
                "Market cap, circulating supply, and chain distribution for {name}. \
                 Peg type: {peg_type}. Available on {} chains.",
                chains.len()
            )),
            tags: {
                let mut t = vec![
                    "stablecoin".into(),
                    symbol.to_lowercase(),
                    peg_type.into(),
                    "defi".into(),
                    "free".into(),
                ];
                for c in &chains {
                    t.push(c.to_lowercase());
                }
                t
            },
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
            provider: Did("source:defillama".into()),
            source: DataSource::DefiLlama,
            market: None,
            data_type: DataType::Tabular,
            created_at: chrono::Utc::now(),
            seller_endpoint: None,
            source_attributes: Some(serde_json::json!({
                "chain": chains.first().unwrap_or(&"multi-chain".to_string()),
                "chains": chains,
                "protocol": symbol.to_lowercase(),
                "token_symbols": [symbol],
                "category": "stablecoin",
                "peg_type": peg_type,
                "refresh_cadence": "daily",
                "origin_url": format!("https://defillama.com/stablecoin/{name}"),
                "api_url": format!("https://stablecoins.llama.fi/stablecoin/{id}"),
                "is_open_data": true,
                "defillama_id": id,
            })),
            provider_meta: None,
            governance: None,
        })
    }

    async fn search_bridges(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let resp: serde_json::Value = self
            .client
            .get(DEFILLAMA_BRIDGES_URL)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let empty = vec![];
        let bridges = resp
            .get("bridges")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        Ok(bridges
            .iter()
            .filter(|b| {
                let name = b
                    .get("displayName")
                    .or_else(|| b.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                name.to_lowercase().contains(query) || query.contains("bridge")
            })
            .take(limit)
            .filter_map(|bridge| {
                let name = bridge
                    .get("displayName")
                    .or_else(|| bridge.get("name"))?
                    .as_str()?;
                let id = match bridge.get("id")? {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => return None,
                };
                let chains: Vec<String> = bridge
                    .get("chains")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|c| c.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                Some(SearchResult {
                    cid: DatasetCid(format!("defillama:bridge:{id}")),
                    title: format!("{name} — Bridge Volume Data"),
                    description: Some(format!(
                        "Cross-chain bridge volume for {name}. Covers {} chains.",
                        chains.len()
                    )),
                    tags: vec![
                        "bridge".into(),
                        "cross-chain".into(),
                        "defi".into(),
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
                    provider: Did("source:defillama".into()),
                    source: DataSource::DefiLlama,
                    market: None,
                    data_type: DataType::Tabular,
                    created_at: chrono::Utc::now(),
                    seller_endpoint: None,
                    source_attributes: Some(serde_json::json!({
                        "chains": chains,
                        "category": "bridge",
                        "refresh_cadence": "daily",
                        "origin_url": format!("https://defillama.com/bridge/{id}"),
                        "api_url": format!("https://api.llama.fi/v2/bridge/{id}"),
                        "is_open_data": true,
                    })),
                    provider_meta: None,
                    governance: None,
                })
            })
            .collect())
    }

    async fn search_protocols(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let resp: Vec<serde_json::Value> = self
            .client
            .get(DEFILLAMA_PROTOCOLS_URL)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(resp
            .iter()
            .filter(|p| {
                let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let symbol = p.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                let category = p.get("category").and_then(|v| v.as_str()).unwrap_or("");
                name.to_lowercase().contains(query)
                    || symbol.to_lowercase().contains(query)
                    || category.to_lowercase().contains(query)
            })
            .take(limit)
            .filter_map(|proto| {
                let name = proto.get("name")?.as_str()?;
                let slug = proto.get("slug")?.as_str()?;
                let category = proto
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("defi");
                let chains: Vec<String> = proto
                    .get("chains")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|c| c.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                Some(SearchResult {
                    cid: DatasetCid(format!("defillama:protocol:{slug}")),
                    title: format!("{name} — DeFi Protocol TVL Data"),
                    description: Some(format!("{name} TVL and metrics. Category: {category}.")),
                    tags: vec![
                        category.to_lowercase(),
                        "defi".into(),
                        "tvl".into(),
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
                    provider: Did("source:defillama".into()),
                    source: DataSource::DefiLlama,
                    market: None,
                    data_type: DataType::Tabular,
                    created_at: chrono::Utc::now(),
                    seller_endpoint: None,
                    source_attributes: Some(serde_json::json!({
                        "chains": chains,
                        "protocol": slug,
                        "category": category.to_lowercase(),
                        "refresh_cadence": "daily",
                        "origin_url": format!("https://defillama.com/protocol/{slug}"),
                        "api_url": format!("https://api.llama.fi/protocol/{slug}"),
                        "is_open_data": true,
                    })),
                    provider_meta: None,
                    governance: None,
                })
            })
            .collect())
    }

    /// Pull full stablecoin catalog (for periodic sync).
    pub async fn fetch_full_stablecoin_catalog(&self) -> Result<Vec<SearchResult>> {
        let resp: serde_json::Value = self
            .client
            .get(DEFILLAMA_STABLECOINS_URL)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let empty = vec![];
        let assets = resp
            .get("peggedAssets")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        Ok(assets
            .iter()
            .filter_map(|a| self.stablecoin_to_result(a))
            .collect())
    }
}
