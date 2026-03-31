use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;

use super::util::{infer_data_type_from_title, parse_data_type};
use super::ExternalAdapter;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GuixuHubDatasetResponse {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    data_type: String,
    #[serde(default)]
    schema: GuixuHubSchemaResponse,
    #[serde(default)]
    metrics: GuixuHubMetricsResponse,
    #[serde(default)]
    price: GuixuHubPriceResponse,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    x402_endpoint: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct GuixuHubSchemaResponse {
    #[serde(default)]
    columns: Vec<GuixuHubColumnResponse>,
    #[serde(default)]
    row_count: u64,
    #[serde(default)]
    size_bytes: u64,
}

#[derive(Debug, Default, Deserialize)]
struct GuixuHubColumnResponse {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct GuixuHubMetricsResponse {
    #[serde(default)]
    download_count: u64,
    #[serde(default)]
    review_count: u64,
    #[serde(default)]
    trade_count: u64,
}

#[derive(Debug, Deserialize)]
struct GuixuHubPriceResponse {
    #[serde(default)]
    amount: f64,
    #[serde(default = "default_guixu_hub_currency")]
    currency: String,
}

impl Default for GuixuHubPriceResponse {
    fn default() -> Self {
        Self {
            amount: 0.0,
            currency: default_guixu_hub_currency(),
        }
    }
}

fn default_guixu_hub_currency() -> String {
    "ETH".to_string()
}

pub struct GuixuHubAdapter {
    client: reqwest::Client,
    api_url: String,
}

impl Default for GuixuHubAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_url: std::env::var("GUIXU_HUB_API_URL")
                .unwrap_or_else(|_| "https://www.guixu.org/api/hub/datasets".into()),
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for GuixuHubAdapter {
    fn name(&self) -> &str {
        "guixu_hub"
    }

    fn source_type(&self) -> DataSource {
        DataSource::GuixuHub
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let limit_str = limit.min(100).to_string();
        let items = tokio::time::timeout(
            Duration::from_secs(10),
            self.client
                .get(&self.api_url)
                .query(&[("q", query), ("limit", &limit_str)])
                .send(),
        )
        .await
        .map_err(|_| anyhow!("guixu hub: timeout"))??
        .error_for_status()?
        .json::<Vec<GuixuHubDatasetResponse>>()
        .await?;

        Ok(items
            .into_iter()
            .take(limit)
            .map(|item| {
                let data_type = parse_data_type(&item.data_type)
                    .unwrap_or_else(|| infer_data_type_from_title(&item.title));
                let created_at = chrono::DateTime::parse_from_rfc3339(&item.created_at)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());

                SearchResult {
                    cid: DatasetCid(format!("guixu-hub:{}", item.id)),
                    title: item.title,
                    description: if item.description.trim().is_empty() {
                        None
                    } else {
                        Some(item.description)
                    },
                    tags: item.tags,
                    schema: DatasetSchema {
                        columns: item
                            .schema
                            .columns
                            .into_iter()
                            .filter_map(|column| {
                                let name = column.name.trim().to_string();
                                if name.is_empty() {
                                    None
                                } else {
                                    Some(ColumnDef {
                                        name,
                                        dtype: "unknown".into(),
                                        nullable: true,
                                        description: None,
                                    })
                                }
                            })
                            .collect(),
                        row_count: item.schema.row_count,
                        size_bytes: item.schema.size_bytes,
                    },
                    quality: None,
                    price: Price {
                        amount: item.price.amount,
                        currency: item.price.currency,
                    },
                    license: License {
                        spdx_id: "proprietary".into(),
                        commercial_use: false,
                        derivative_allowed: false,
                    },
                    provider: Did(format!("guixu:hub:{}", item.id)),
                    source: DataSource::GuixuHub,
                    market: Some(DatasetMarketStats {
                        download_count: item.metrics.download_count,
                        review_count: item.metrics.review_count,
                        trade_count: item.metrics.trade_count,
                    }),
                    data_type,
                    created_at,
                    seller_endpoint: item.x402_endpoint.map(|ep| {
                        if ep.starts_with("http") {
                            ep
                        } else {
                            let base = std::env::var("GUIXU_HUB_BASE_URL")
                                .unwrap_or_else(|_| "https://www.guixu.org".into());
                            format!("{base}{ep}")
                        }
                    }),
                    source_attributes: None,
                }
            })
            .collect())
    }
}
