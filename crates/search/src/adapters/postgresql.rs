// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Result;
use data_core::config::PostgreSqlCatalog;
use data_core::types::*;
use tokio_postgres::NoTls;

use super::ExternalAdapter;

/// Searches table/column metadata across configured PostgreSQL databases.
#[derive(Default)]
pub struct PostgreSqlAdapter {
    catalogs: Vec<PostgreSqlCatalog>,
}

impl PostgreSqlAdapter {
    pub fn with_catalogs(catalogs: Vec<PostgreSqlCatalog>) -> Self {
        Self { catalogs }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for PostgreSqlAdapter {
    fn name(&self) -> &str {
        "postgresql"
    }
    fn source_type(&self) -> DataSource {
        DataSource::PostgreSql
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
            match search_catalog(catalog, &keywords, limit - results.len()).await {
                Ok(mut rows) => results.append(&mut rows),
                Err(e) => {
                    tracing::warn!(
                        label = %catalog.label,
                        error = %e,
                        "postgresql catalog search failed"
                    );
                }
            }
        }

        Ok(results)
    }
}

async fn search_catalog(
    catalog: &PostgreSqlCatalog,
    keywords: &[&str],
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let (client, connection) = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_postgres::connect(&catalog.url, NoTls),
    )
    .await
    .map_err(|_| anyhow::anyhow!("postgresql connect timeout"))??;

    // Drive the connection in background
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::warn!(error = %e, "postgresql connection error");
        }
    });

    // Build schema filter
    let schema_filter = if catalog.schemas.is_empty() {
        "table_schema NOT IN ('information_schema', 'pg_catalog', 'pg_toast')".to_string()
    } else {
        let list: Vec<String> = catalog.schemas.iter().map(|s| format!("'{s}'")).collect();
        format!("table_schema IN ({})", list.join(", "))
    };

    let sql = format!(
        "SELECT table_schema, table_name, table_type \
         FROM information_schema.tables \
         WHERE {schema_filter}"
    );

    let rows = client.query(&sql, &[]).await?;

    let mut results = Vec::new();

    for row in &rows {
        if results.len() >= limit {
            break;
        }

        let schema: &str = row.get(0);
        let table: &str = row.get(1);
        let obj_type: &str = row.get(2);

        // Fetch columns
        let col_rows = client
            .query(
                "SELECT column_name, data_type \
                 FROM information_schema.columns \
                 WHERE table_schema = $1 AND table_name = $2 \
                 ORDER BY ordinal_position",
                &[&schema, &table],
            )
            .await
            .unwrap_or_default();

        let columns: Vec<(String, String)> = col_rows
            .iter()
            .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
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

        let cid = format!("postgresql:{}:{}.{}", catalog.label, schema, table);

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
                "[{}] PostgreSQL table — {} columns",
                catalog.label,
                col_schema.len(),
            )),
            tags: vec!["postgresql".into(), catalog.label.clone()],
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
            provider: Did(format!("postgresql:{}", catalog.label)),
            source: DataSource::PostgreSql,
            market: None,
            data_type: DataType::Tabular,
            created_at: chrono::Utc::now(),
            seller_endpoint: None,
            source_attributes: Some(attrs),
        });
    }

    Ok(results)
}
