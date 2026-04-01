use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;

use super::ExternalAdapter;

pub struct ArxivAdapter {
    client: reqwest::Client,
}

impl Default for ArxivAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for ArxivAdapter {
    fn name(&self) -> &str {
        "arxiv"
    }
    fn source_type(&self) -> DataSource {
        DataSource::Arxiv
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let max_results = limit.min(100).to_string();
        // arXiv API uses Atom XML; search_query supports all: prefix for general search
        let search_query = format!("all:{query}");
        let resp = tokio::time::timeout(
            Duration::from_secs(15),
            self.client
                .get("https://export.arxiv.org/api/query")
                .query(&[
                    ("search_query", search_query.as_str()),
                    ("start", "0"),
                    ("max_results", &max_results),
                    ("sortBy", "relevance"),
                    ("sortOrder", "descending"),
                ])
                .send(),
        )
        .await
        .map_err(|_| anyhow!("arxiv: timeout"))??
        .error_for_status()?
        .text()
        .await?;

        parse_atom_feed(&resp, limit)
    }
}

/// Minimal Atom XML parser for arXiv results — avoids pulling in a full XML crate.
fn parse_atom_feed(xml: &str, limit: usize) -> Result<Vec<SearchResult>> {
    let mut results = Vec::new();
    // Split on <entry> tags
    for entry_xml in xml.split("<entry>").skip(1).take(limit) {
        let title = extract_tag(entry_xml, "title")
            .map(|s| s.replace('\n', " ").trim().to_string())
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let summary = extract_tag(entry_xml, "summary")
            .map(|s| s.replace('\n', " ").trim().to_string())
            .unwrap_or_default();
        let published = extract_tag(entry_xml, "published").unwrap_or_default();
        let arxiv_id = extract_tag(entry_xml, "id").unwrap_or_default();

        // Extract primary category
        let category =
            extract_attr(entry_xml, "arxiv:primary_category", "term").unwrap_or_default();

        // Extract authors
        let authors = extract_authors(entry_xml);

        // Extract PDF link
        let pdf_link = extract_pdf_link(entry_xml);

        // Extract DOI if present
        let doi = extract_tag(entry_xml, "arxiv:doi").unwrap_or_default();

        let mut desc_parts = vec![];
        if !authors.is_empty() {
            desc_parts.push(authors.clone());
        }
        if !summary.is_empty() {
            desc_parts.push(truncate(&summary, 400));
        }
        if !pdf_link.is_empty() {
            desc_parts.push(pdf_link.clone());
        }

        let cid = if !doi.is_empty() {
            doi.clone()
        } else {
            arxiv_id.clone()
        };

        let created_at = chrono::DateTime::parse_from_rfc3339(published.trim())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());

        results.push(SearchResult {
            cid: DatasetCid(cid),
            title,
            description: Some(desc_parts.join(". ")),
            tags: if category.is_empty() {
                vec![]
            } else {
                vec![category]
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
            provider: Did(arxiv_id),
            source: DataSource::Arxiv,
            market: None,
            data_type: DataType::Text,
            created_at,
            seller_endpoint: None,
            source_attributes: None,
        });
    }
    Ok(results)
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start = xml.find(&open)?;
    let after_open = xml[start..].find('>')? + start + 1;
    let end = xml[after_open..].find(&close)? + after_open;
    Some(xml[after_open..end].to_string())
}

fn extract_attr(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let open = format!("<{tag} ");
    let start = xml.find(&open)?;
    let tag_end = xml[start..].find('>')? + start;
    let tag_str = &xml[start..tag_end];
    let attr_prefix = format!("{attr}=\"");
    let attr_start = tag_str.find(&attr_prefix)? + attr_prefix.len();
    let attr_end = tag_str[attr_start..].find('"')? + attr_start;
    Some(tag_str[attr_start..attr_end].to_string())
}

fn extract_authors(xml: &str) -> String {
    let mut names = Vec::new();
    for chunk in xml.split("<author>").skip(1) {
        if let Some(name) = extract_tag(chunk, "name") {
            names.push(name.trim().to_string());
        }
    }
    match names.len() {
        0 => String::new(),
        1 => names[0].clone(),
        2 => format!("{} and {}", names[0], names[1]),
        _ => format!("{}, {} et al.", names[0], names[1]),
    }
}

fn extract_pdf_link(xml: &str) -> String {
    // Look for <link ... type="application/pdf" ... href="..."/>
    for chunk in xml.split("<link ").skip(1) {
        let end = chunk
            .find("/>")
            .or_else(|| chunk.find('>'))
            .unwrap_or(chunk.len());
        let tag = &chunk[..end];
        if tag.contains("application/pdf") {
            if let Some(href) = extract_href(tag) {
                return href;
            }
        }
    }
    String::new()
}

fn extract_href(tag: &str) -> Option<String> {
    let prefix = "href=\"";
    let start = tag.find(prefix)? + prefix.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
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
