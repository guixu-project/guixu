// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::config::DuckDbCatalog;
use data_core::types::*;

use super::ExternalAdapter;

/// Searches DuckDB table metadata via its HTTP API.
/// Requires DuckDB to be running with `duckdb -httpd -port <port>`.
pub struct DuckDbAdapter {
    catalogs: Vec<DuckDbCatalog>,
    client: reqwest::Client,
}

impl Default for DuckDbAdapter {
    fn default() -> Self {
        Self {
            catalogs: vec![],
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

impl DuckDbAdapter {
    pub fn with_catalogs(catalogs: Vec<DuckDbCatalog>) -> Self {
        Self {
            catalogs,
            ..Self::default()
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for DuckDbAdapter {
    fn name(&self) -> &str {
        "duckdb"
    }
    fn source_type(&self) -> DataSource {
        DataSource::DuckDb
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if self.catalogs.is_empty() {
            return Ok(vec![]);
        }

        let query_lower = query.to_lowercase();
        let keywords: Vec<&str> = query_lower.split_whitespace().collect();
        let mut results = Vec::new();

        for catalog in &self.catalogs {
            if results.len() >= limit {
                break;
            }
            match search_catalog(&self.client, catalog, &keywords, limit - results.len()).await {
                Ok(mut rows) => results.append(&mut rows),
                Err(e) => {
                    tracing::warn!(
                        label = %catalog.label,
                        error = %e,
                        "duckdb http search failed"
                    );
                }
            }
        }

        Ok(results)
    }
}

async fn search_catalog(
    client: &reqwest::Client,
    catalog: &DuckDbCatalog,
    keywords: &[&str],
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let url = &catalog.url;

    // Fetch tables
    let tables = execute_sql(
        client,
        url,
        "SELECT table_schema, table_name, table_type \
         FROM information_schema.tables \
         WHERE table_schema NOT IN ('information_schema', 'pg_catalog')",
    )
    .await?;

    let mut results = Vec::new();

    for row in &tables {
        if results.len() >= limit {
            break;
        }

        let schema = row.first().cloned().unwrap_or_default();
        let table = row.get(1).cloned().unwrap_or_default();
        let obj_type = row.get(2).cloned().unwrap_or_default();

        // Fetch columns
        let col_rows = execute_sql(
            client,
            url,
            &format!(
                "SELECT column_name, data_type FROM information_schema.columns \
             WHERE table_schema = '{schema}' AND table_name = '{table}' \
             ORDER BY ordinal_position"
            ),
        )
        .await
        .unwrap_or_default();

        let columns: Vec<(String, String)> = col_rows
            .into_iter()
            .filter_map(|r| {
                let mut it = r.into_iter();
                Some((it.next()?, it.next().unwrap_or_default()))
            })
            .collect();

        let col_names: Vec<&str> = columns.iter().map(|(n, _)| n.as_str()).collect();
        let haystack = format!(
            "{} {} {}",
            table.to_lowercase(),
            col_names.join(" ").to_lowercase(),
            schema.to_lowercase(),
        );

        if !keywords.iter().all(|kw| haystack.contains(kw)) {
            continue;
        }

        let col_schema: Vec<ColumnDef> = columns
            .iter()
            .map(|(name, dtype)| ColumnDef {
                name: name.clone(),
                dtype: dtype.clone(),
                nullable: true,
                description: None,
            })
            .collect();

        let cid = format!("duckdb:{}:{}.{}", catalog.label, schema, table);

        let attrs = serde_json::json!({
            "catalog_label": catalog.label,
            "schema": schema,
            "table": table,
            "object_type": obj_type,
            "is_external_db": true,
        });

        results.push(SearchResult {
            cid: DatasetCid(cid),
            title: format!("{schema}.{table}"),
            description: Some(format!(
                "[{}] DuckDB table — {} columns",
                catalog.label,
                col_schema.len(),
            )),
            tags: vec!["duckdb".into(), catalog.label.clone()],
            schema: DatasetSchema {
                columns: col_schema,
                row_count: 0,
                size_bytes: 0,
            },
            quality: None,
            price: Price::free(),
            license: License {
                spdx_id: "proprietary".into(),
                commercial_use: false,
                derivative_allowed: false,
            },
            provider: Did(format!("duckdb:{}", catalog.label)),
            source: DataSource::DuckDb,
            market: None,
            data_type: DataType::Tabular,
            created_at: chrono::Utc::now(),
            seller_endpoint: None,
            source_attributes: Some(attrs),
        });
    }

    Ok(results)
}

/// Execute SQL via DuckDB HTTP API (POST /query with JSON body).
async fn execute_sql(
    client: &reqwest::Client,
    base_url: &str,
    sql: &str,
) -> Result<Vec<Vec<String>>> {
    let url = format!("{}/query", base_url.trim_end_matches('/'));

    let resp: serde_json::Value = client
        .post(&url)
        .json(&serde_json::json!({ "sql": sql }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
        return Err(anyhow!("duckdb: {err}"));
    }

    let empty = vec![];
    let data = resp
        .get("data")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    Ok(data
        .iter()
        .filter_map(|row| {
            row.as_array().map(|cols| {
                cols.iter()
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect()
            })
        })
        .collect())
}
