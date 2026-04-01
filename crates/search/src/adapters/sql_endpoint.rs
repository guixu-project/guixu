use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::config::{SqlEndpointCatalog, SqlEngine};
use data_core::types::*;

use super::ExternalAdapter;

/// Searches table metadata via SQL-over-HTTP endpoints (Presto/Trino, Spark, Flink).
pub struct SqlEndpointAdapter {
    catalogs: Vec<SqlEndpointCatalog>,
    client: reqwest::Client,
}

impl Default for SqlEndpointAdapter {
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

impl SqlEndpointAdapter {
    pub fn with_catalogs(catalogs: Vec<SqlEndpointCatalog>) -> Self {
        Self {
            catalogs,
            ..Self::default()
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for SqlEndpointAdapter {
    fn name(&self) -> &str {
        "sql_endpoint"
    }
    fn source_type(&self) -> DataSource {
        // Returns the first configured engine's source, or Presto as default.
        self.catalogs
            .first()
            .map(|c| engine_to_source(c.engine))
            .unwrap_or(DataSource::Presto)
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
            match search_endpoint(&self.client, catalog, &keywords, limit - results.len()).await {
                Ok(mut rows) => results.append(&mut rows),
                Err(e) => {
                    tracing::warn!(
                        label = %catalog.label,
                        engine = ?catalog.engine,
                        error = %e,
                        "sql endpoint search failed"
                    );
                }
            }
        }

        Ok(results)
    }
}

fn engine_to_source(engine: SqlEngine) -> DataSource {
    match engine {
        SqlEngine::Presto => DataSource::Presto,
        SqlEngine::Spark => DataSource::Spark,
        SqlEngine::Flink => DataSource::Flink,
    }
}

async fn search_endpoint(
    client: &reqwest::Client,
    catalog: &SqlEndpointCatalog,
    keywords: &[&str],
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let tables = fetch_tables(client, catalog).await?;
    let source = engine_to_source(catalog.engine);
    let engine_name = format!("{:?}", catalog.engine).to_lowercase();

    let mut results = Vec::new();

    for (schema, table, columns) in &tables {
        if results.len() >= limit {
            break;
        }

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

        let cid = format!("{}:{}:{}.{}", engine_name, catalog.label, schema, table);

        let attrs = serde_json::json!({
            "catalog_label": catalog.label,
            "engine": engine_name,
            "schema": schema,
            "table": table,
            "is_external_db": true,
        });

        results.push(SearchResult {
            cid: DatasetCid(cid),
            title: format!("{schema}.{table}"),
            description: Some(format!(
                "[{}] {} table — {} columns",
                catalog.label,
                engine_name,
                col_schema.len(),
            )),
            tags: vec![engine_name.clone(), catalog.label.clone()],
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
            provider: Did(format!("{}:{}", engine_name, catalog.label)),
            source: source.clone(),
            market: None,
            data_type: DataType::Tabular,
            created_at: chrono::Utc::now(),
            seller_endpoint: None,
            source_attributes: Some(attrs),
        });
    }

    Ok(results)
}

/// Fetches tables and their columns via Presto/Trino REST API.
/// Spark Thrift (HTTP mode) and Flink SQL Gateway use compatible protocols.
async fn fetch_tables(
    client: &reqwest::Client,
    catalog: &SqlEndpointCatalog,
) -> Result<Vec<(String, String, Vec<(String, String)>)>> {
    let schemas = if catalog.schemas.is_empty() {
        fetch_schemas(client, catalog).await?
    } else {
        catalog.schemas.clone()
    };

    let mut tables = Vec::new();

    for schema in &schemas {
        let sql = match catalog.engine {
            SqlEngine::Presto => {
                if let Some(cat) = &catalog.catalog {
                    format!("SHOW TABLES FROM {cat}.{schema}")
                } else {
                    format!("SHOW TABLES FROM {schema}")
                }
            }
            SqlEngine::Spark | SqlEngine::Flink => {
                format!("SHOW TABLES IN {schema}")
            }
        };

        let rows = execute_query(client, catalog, &sql)
            .await
            .unwrap_or_default();

        for row in &rows {
            let table_name = row.first().cloned().unwrap_or_default();
            if table_name.is_empty() {
                continue;
            }

            let columns = fetch_columns(client, catalog, schema, &table_name)
                .await
                .unwrap_or_default();

            tables.push((schema.clone(), table_name, columns));
        }
    }

    Ok(tables)
}

async fn fetch_schemas(
    client: &reqwest::Client,
    catalog: &SqlEndpointCatalog,
) -> Result<Vec<String>> {
    let sql = match catalog.engine {
        SqlEngine::Presto => {
            if let Some(cat) = &catalog.catalog {
                format!("SHOW SCHEMAS FROM {cat}")
            } else {
                "SHOW SCHEMAS".to_string()
            }
        }
        SqlEngine::Spark => "SHOW DATABASES".to_string(),
        SqlEngine::Flink => "SHOW DATABASES".to_string(),
    };

    let rows = execute_query(client, catalog, &sql).await?;
    let skip = ["information_schema", "pg_catalog", "sys", "system"];

    Ok(rows
        .into_iter()
        .filter_map(|r| r.into_iter().next())
        .filter(|s| !skip.contains(&s.as_str()))
        .collect())
}

async fn fetch_columns(
    client: &reqwest::Client,
    catalog: &SqlEndpointCatalog,
    schema: &str,
    table: &str,
) -> Result<Vec<(String, String)>> {
    let fqn = match (&catalog.engine, &catalog.catalog) {
        (SqlEngine::Presto, Some(cat)) => format!("{cat}.{schema}.{table}"),
        _ => format!("{schema}.{table}"),
    };
    let sql = format!("DESCRIBE {fqn}");
    let rows = execute_query(client, catalog, &sql).await?;

    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let mut it = r.into_iter();
            let name = it.next()?;
            let dtype = it.next().unwrap_or_default();
            Some((name, dtype))
        })
        .collect())
}

/// Execute a SQL statement via Presto/Trino-compatible REST API.
/// POST /v1/statement with body = SQL, then poll nextUri until done.
async fn execute_query(
    client: &reqwest::Client,
    catalog: &SqlEndpointCatalog,
    sql: &str,
) -> Result<Vec<Vec<String>>> {
    let url = format!("{}/v1/statement", catalog.url.trim_end_matches('/'));

    let mut resp: serde_json::Value = client
        .post(&url)
        .header("X-Presto-User", "guixu")
        .header("X-Trino-User", "guixu")
        .body(sql.to_string())
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let mut all_data: Vec<Vec<String>> = Vec::new();

    loop {
        // Collect data from this response
        if let Some(data) = resp.get("data").and_then(|v| v.as_array()) {
            for row in data {
                if let Some(cols) = row.as_array() {
                    let string_row: Vec<String> = cols
                        .iter()
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect();
                    all_data.push(string_row);
                }
            }
        }

        // Check for error
        if let Some(err) = resp.get("error") {
            let msg = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(anyhow!("query error: {msg}"));
        }

        // Follow nextUri or finish
        let next = resp
            .get("nextUri")
            .and_then(|v| v.as_str())
            .map(String::from);

        match next {
            Some(next_url) => {
                tokio::time::sleep(Duration::from_millis(100)).await;
                resp = client
                    .get(&next_url)
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
            }
            None => break,
        }
    }

    Ok(all_data)
}
