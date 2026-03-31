use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;

use super::ExternalAdapter;

pub struct DblpAdapter {
    client: reqwest::Client,
}

impl Default for DblpAdapter {
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
impl ExternalAdapter for DblpAdapter {
    fn name(&self) -> &str {
        "dblp"
    }
    fn source_type(&self) -> DataSource {
        DataSource::Dblp
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let h = limit.min(100).to_string();
        let resp = tokio::time::timeout(
            Duration::from_secs(10),
            self.client
                .get("https://dblp.org/search/publ/api")
                .query(&[("q", query), ("format", "json"), ("h", &h), ("c", "0")])
                .send(),
        )
        .await
        .map_err(|_| anyhow!("dblp: timeout"))??
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

        let empty = vec![];
        let hits = resp
            .pointer("/result/hits/hit")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        Ok(hits
            .iter()
            .take(limit)
            .filter_map(|hit| {
                let info = hit.get("info")?;
                let title = info.get("title").and_then(|v| v.as_str())?;
                let year = info.get("year").and_then(|v| v.as_str()).unwrap_or("");
                let venue = info.get("venue").and_then(|v| v.as_str()).unwrap_or("");
                let doi = info.get("doi").and_then(|v| v.as_str()).unwrap_or("");
                let url = info.get("ee").and_then(|v| v.as_str()).unwrap_or("");
                let key = info.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let pub_type = info.get("type").and_then(|v| v.as_str()).unwrap_or("");

                let authors = extract_authors(info);

                let description = format!(
                    "{authors} ({year}). {venue}. {pub_type}{}",
                    if url.is_empty() {
                        String::new()
                    } else {
                        format!("\n{url}")
                    }
                );

                let cid = if !doi.is_empty() {
                    doi.to_string()
                } else {
                    format!("dblp:{key}")
                };

                Some(SearchResult {
                    cid: DatasetCid(cid),
                    title: title.to_string(),
                    description: Some(description),
                    tags: vec![venue.to_string()],
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
                    provider: Did(format!("dblp:{key}")),
                    source: DataSource::Dblp,
                    market: None,
                    data_type: DataType::Text,
                    created_at: parse_year(year),
                    seller_endpoint: None,
                    source_attributes: None,
                })
            })
            .collect())
    }
}

fn extract_authors(info: &serde_json::Value) -> String {
    let authors_val = match info.pointer("/authors/author") {
        Some(v) => v,
        None => return String::new(),
    };

    let names: Vec<&str> = if let Some(arr) = authors_val.as_array() {
        arr.iter()
            .filter_map(|a| a.get("text").and_then(|v| v.as_str()))
            .collect()
    } else {
        // Single author — not wrapped in array
        authors_val
            .get("text")
            .and_then(|v| v.as_str())
            .into_iter()
            .collect()
    };

    match names.len() {
        0 => String::new(),
        1 => names[0].to_string(),
        2 => format!("{} and {}", names[0], names[1]),
        _ => format!("{}, {} et al.", names[0], names[1]),
    }
}

fn parse_year(year: &str) -> chrono::DateTime<chrono::Utc> {
    year.parse::<i32>()
        .ok()
        .and_then(|y| {
            chrono::NaiveDate::from_ymd_opt(y, 1, 1)
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                .map(|dt| dt.and_utc())
        })
        .unwrap_or_else(chrono::Utc::now)
}
