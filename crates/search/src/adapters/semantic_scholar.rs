use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;

use super::ExternalAdapter;

pub struct SemanticScholarAdapter {
    client: reqwest::Client,
}

impl Default for SemanticScholarAdapter {
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
impl ExternalAdapter for SemanticScholarAdapter {
    fn name(&self) -> &str {
        "semantic_scholar"
    }
    fn source_type(&self) -> DataSource {
        DataSource::SemanticScholar
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let limit_str = limit.min(100).to_string();
        let fields =
            "paperId,title,abstract,authors,year,venue,citationCount,openAccessPdf,externalIds";
        let resp = tokio::time::timeout(
            Duration::from_secs(10),
            self.client
                .get("https://api.semanticscholar.org/graph/v1/paper/search")
                .query(&[("query", query), ("limit", &limit_str), ("fields", fields)])
                .send(),
        )
        .await
        .map_err(|_| anyhow!("semantic_scholar: timeout"))??
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

        let empty = vec![];
        let papers = resp
            .get("data")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        Ok(papers
            .iter()
            .take(limit)
            .filter_map(|p| {
                let title = p.get("title").and_then(|v| v.as_str())?;
                let paper_id = p.get("paperId").and_then(|v| v.as_str()).unwrap_or("");
                let year = p.get("year").and_then(|v| v.as_u64()).unwrap_or(0);
                let venue = p.get("venue").and_then(|v| v.as_str()).unwrap_or("");
                let abstract_text = p.get("abstract").and_then(|v| v.as_str()).unwrap_or("");
                let citations = p.get("citationCount").and_then(|v| v.as_u64()).unwrap_or(0);

                let authors = format_authors(p);
                let doi = p
                    .pointer("/externalIds/DOI")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let pdf_url = p
                    .pointer("/openAccessPdf/url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let year_str = if year > 0 {
                    year.to_string()
                } else {
                    String::new()
                };

                let mut desc_parts = vec![];
                if !authors.is_empty() {
                    desc_parts.push(format!("{authors} ({year_str})"));
                }
                if !venue.is_empty() {
                    desc_parts.push(venue.to_string());
                }
                if citations > 0 {
                    desc_parts.push(format!("Citations: {citations}"));
                }
                if !abstract_text.is_empty() {
                    let trunc = truncate(abstract_text, 300);
                    desc_parts.push(trunc);
                }
                if !pdf_url.is_empty() {
                    desc_parts.push(pdf_url.to_string());
                }

                let cid = if !doi.is_empty() {
                    doi.to_string()
                } else {
                    format!("s2:{paper_id}")
                };

                Some(SearchResult {
                    cid: DatasetCid(cid),
                    title: title.to_string(),
                    description: Some(desc_parts.join(". ")),
                    tags: if venue.is_empty() {
                        vec![]
                    } else {
                        vec![venue.to_string()]
                    },
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
                    provider: Did(format!("s2:{paper_id}")),
                    source: DataSource::SemanticScholar,
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

fn format_authors(paper: &serde_json::Value) -> String {
    let empty = vec![];
    let authors = paper
        .get("authors")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    let names: Vec<&str> = authors
        .iter()
        .filter_map(|a| a.get("name").and_then(|v| v.as_str()))
        .collect();
    match names.len() {
        0 => String::new(),
        1 => names[0].to_string(),
        2 => format!("{} and {}", names[0], names[1]),
        _ => format!("{}, {} et al.", names[0], names[1]),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let end = s
            .char_indices()
            .take_while(|(i, _)| *i < max)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(max);
        format!("{}…", &s[..end])
    }
}

fn parse_year(year: u64) -> chrono::DateTime<chrono::Utc> {
    if year == 0 {
        return chrono::Utc::now();
    }
    chrono::NaiveDate::from_ymd_opt(year as i32, 1, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc())
        .unwrap_or_else(chrono::Utc::now)
}
