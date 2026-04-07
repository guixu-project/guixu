// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;
use serde::Deserialize;

use super::ExternalAdapter;

// ---------------------------------------------------------------------------
// SqlExecutor trait — abstracts over connection protocols
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
trait SqlExecutor: Send + Sync {
    async fn execute(&self, sql: &str) -> Result<Vec<Vec<String>>>;
}

// ---------------------------------------------------------------------------
// PostgreSQL executor (native TCP via tokio-postgres)
// ---------------------------------------------------------------------------

struct PostgreSqlExecutor {
    url: String,
}

#[async_trait::async_trait]
impl SqlExecutor for PostgreSqlExecutor {
    async fn execute(&self, sql: &str) -> Result<Vec<Vec<String>>> {
        let (client, connection) = tokio::time::timeout(
            Duration::from_secs(5),
            tokio_postgres::connect(&self.url, tokio_postgres::NoTls),
        )
        .await
        .map_err(|_| anyhow!("postgresql connect timeout"))??;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::warn!(error = %e, "postgresql connection error");
            }
        });

        let rows = client.query(sql, &[]).await?;
        Ok(rows
            .iter()
            .map(|row| {
                (0..row.len())
                    .map(|i| {
                        row.try_get::<_, String>(i)
                            .unwrap_or_else(|_| String::new())
                    })
                    .collect()
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// DuckDB executor (HTTP POST /query)
// ---------------------------------------------------------------------------

struct DuckDbExecutor {
    url: String,
    client: reqwest::Client,
}

#[async_trait::async_trait]
impl SqlExecutor for DuckDbExecutor {
    async fn execute(&self, sql: &str) -> Result<Vec<Vec<String>>> {
        let url = format!("{}/query", self.url.trim_end_matches('/'));
        let resp: serde_json::Value = self
            .client
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
}

// ---------------------------------------------------------------------------
// Presto/Trino/Spark/Flink executor (REST API with polling)
// ---------------------------------------------------------------------------

struct PrestoExecutor {
    url: String,
    client: reqwest::Client,
}

#[async_trait::async_trait]
impl SqlExecutor for PrestoExecutor {
    async fn execute(&self, sql: &str) -> Result<Vec<Vec<String>>> {
        let url = format!("{}/v1/statement", self.url.trim_end_matches('/'));
        let mut resp: serde_json::Value = self
            .client
            .post(&url)
            .header("X-Presto-User", "guixu")
            .header("X-Trino-User", "guixu")
            .body(sql.to_string())
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let mut all_data = Vec::new();
        loop {
            if let Some(data) = resp.get("data").and_then(|v| v.as_array()) {
                for row in data {
                    if let Some(cols) = row.as_array() {
                        all_data.push(
                            cols.iter()
                                .map(|v| match v {
                                    serde_json::Value::String(s) => s.clone(),
                                    other => other.to_string(),
                                })
                                .collect(),
                        );
                    }
                }
            }
            if let Some(err) = resp.get("error") {
                let msg = err
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                return Err(anyhow!("query error: {msg}"));
            }
            match resp
                .get("nextUri")
                .and_then(|v| v.as_str())
                .map(String::from)
            {
                Some(next_url) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    resp = self
                        .client
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
}

// ---------------------------------------------------------------------------
// Catalog entry — deserialized from skill JSON
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogEntry {
    pub label: String,
    pub url: String,
    #[serde(default)]
    pub schemas: Vec<String>,
    #[serde(default)]
    pub catalog: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SqlCatalogEngine {
    Postgresql,
    Duckdb,
    Presto,
    Spark,
    Flink,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CatalogSource {
    #[default]
    FromConfig,
    Inline {
        entries: Vec<CatalogEntry>,
    },
}

// ---------------------------------------------------------------------------
// SqlCatalogAdapter — unified adapter for all SQL-based catalog search
// ---------------------------------------------------------------------------

pub struct SqlCatalogAdapter {
    skill_id: String,
    skill_name: String,
    engine: SqlCatalogEngine,
    entries: Vec<CatalogEntry>,
    governance: Option<GovernanceMeta>,
    tags: Vec<String>,
}

impl SqlCatalogAdapter {
    pub fn new(
        skill_id: String,
        skill_name: String,
        engine: SqlCatalogEngine,
        entries: Vec<CatalogEntry>,
        governance: Option<GovernanceMeta>,
        tags: Vec<String>,
    ) -> Self {
        Self {
            skill_id,
            skill_name,
            engine,
            entries,
            governance,
            tags,
        }
    }

    fn engine_name(&self) -> &str {
        match self.engine {
            SqlCatalogEngine::Postgresql => "postgresql",
            SqlCatalogEngine::Duckdb => "duckdb",
            SqlCatalogEngine::Presto => "presto",
            SqlCatalogEngine::Spark => "spark",
            SqlCatalogEngine::Flink => "flink",
        }
    }

    fn data_source(&self) -> DataSource {
        match self.engine {
            SqlCatalogEngine::Postgresql => DataSource::PostgreSql,
            SqlCatalogEngine::Duckdb => DataSource::DuckDb,
            SqlCatalogEngine::Presto => DataSource::Presto,
            SqlCatalogEngine::Spark => DataSource::Spark,
            SqlCatalogEngine::Flink => DataSource::Flink,
        }
    }

    fn make_executor(&self, entry: &CatalogEntry) -> Box<dyn SqlExecutor> {
        match self.engine {
            SqlCatalogEngine::Postgresql => Box::new(PostgreSqlExecutor {
                url: entry.url.clone(),
            }),
            SqlCatalogEngine::Duckdb => Box::new(DuckDbExecutor {
                url: entry.url.clone(),
                client: reqwest::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new()),
            }),
            SqlCatalogEngine::Presto | SqlCatalogEngine::Spark | SqlCatalogEngine::Flink => {
                Box::new(PrestoExecutor {
                    url: entry.url.clone(),
                    client: reqwest::Client::builder()
                        .timeout(Duration::from_secs(10))
                        .build()
                        .unwrap_or_else(|_| reqwest::Client::new()),
                })
            }
        }
    }

    fn tables_sql(&self, entry: &CatalogEntry) -> String {
        match self.engine {
            SqlCatalogEngine::Postgresql => {
                let filter = if entry.schemas.is_empty() {
                    "table_schema NOT IN ('information_schema', 'pg_catalog', 'pg_toast')"
                        .to_string()
                } else {
                    let list: Vec<String> =
                        entry.schemas.iter().map(|s| format!("'{s}'")).collect();
                    format!("table_schema IN ({})", list.join(", "))
                };
                format!(
                    "SELECT table_schema, table_name, table_type \
                     FROM information_schema.tables WHERE {filter}"
                )
            }
            SqlCatalogEngine::Duckdb => "SELECT table_schema, table_name, table_type \
                 FROM information_schema.tables \
                 WHERE table_schema NOT IN ('information_schema', 'pg_catalog')"
                .to_string(),
            SqlCatalogEngine::Presto | SqlCatalogEngine::Spark | SqlCatalogEngine::Flink => {
                // For Presto-family, we return a sentinel; actual fetching uses SHOW TABLES
                String::new()
            }
        }
    }

    fn columns_sql(&self, entry: &CatalogEntry, schema: &str, table: &str) -> String {
        match self.engine {
            SqlCatalogEngine::Postgresql | SqlCatalogEngine::Duckdb => {
                format!(
                    "SELECT column_name, data_type FROM information_schema.columns \
                     WHERE table_schema = '{schema}' AND table_name = '{table}' \
                     ORDER BY ordinal_position"
                )
            }
            SqlCatalogEngine::Presto => {
                let fqn = match &entry.catalog {
                    Some(cat) => format!("{cat}.{schema}.{table}"),
                    None => format!("{schema}.{table}"),
                };
                format!("DESCRIBE {fqn}")
            }
            SqlCatalogEngine::Spark | SqlCatalogEngine::Flink => {
                format!("DESCRIBE {schema}.{table}")
            }
        }
    }

    async fn search_entry(
        &self,
        entry: &CatalogEntry,
        keywords: &[&str],
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let executor = self.make_executor(entry);
        let tables = self.fetch_tables(&*executor, entry).await?;
        let source = self.data_source();
        let engine_name = self.engine_name();
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

            let cid = format!("{engine_name}:{}:{schema}.{table}", entry.label);

            results.push(SearchResult {
                cid: DatasetCid(cid),
                title: format!("{schema}.{table}"),
                description: Some(format!(
                    "[{}] {} table — {} columns",
                    entry.label,
                    engine_name,
                    col_schema.len(),
                )),
                tags: vec![engine_name.into(), entry.label.clone()],
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
                provider: Did(format!("{engine_name}:{}", entry.label)),
                source: source.clone(),
                market: None,
                data_type: DataType::Tabular,
                created_at: chrono::Utc::now(),
                seller_endpoint: None,
                source_attributes: Some(serde_json::json!({
                    "catalog_label": entry.label,
                    "engine": engine_name,
                    "schema": schema,
                    "table": table,
                    "is_external_db": true,
                })),
                provider_meta: Some(ProviderMeta {
                    provider_id: self.skill_id.clone(),
                    source_family: SourceFamily::DbCatalog,
                    labels: self.tags.clone(),
                }),
                governance: self.governance.clone(),
            });
        }

        Ok(results)
    }

    async fn fetch_tables(
        &self,
        executor: &dyn SqlExecutor,
        entry: &CatalogEntry,
    ) -> Result<Vec<(String, String, Vec<(String, String)>)>> {
        match self.engine {
            SqlCatalogEngine::Presto | SqlCatalogEngine::Spark | SqlCatalogEngine::Flink => {
                self.fetch_tables_presto_family(executor, entry).await
            }
            _ => self.fetch_tables_information_schema(executor, entry).await,
        }
    }

    async fn fetch_tables_information_schema(
        &self,
        executor: &dyn SqlExecutor,
        entry: &CatalogEntry,
    ) -> Result<Vec<(String, String, Vec<(String, String)>)>> {
        let sql = self.tables_sql(entry);
        let rows = executor.execute(&sql).await?;
        let mut tables = Vec::new();

        for row in &rows {
            let schema = row.first().cloned().unwrap_or_default();
            let table = row.get(1).cloned().unwrap_or_default();

            let col_sql = self.columns_sql(entry, &schema, &table);
            let col_rows = executor.execute(&col_sql).await.unwrap_or_default();
            let columns: Vec<(String, String)> = col_rows
                .into_iter()
                .filter_map(|r| {
                    let mut it = r.into_iter();
                    Some((it.next()?, it.next().unwrap_or_default()))
                })
                .collect();

            tables.push((schema, table, columns));
        }

        Ok(tables)
    }

    async fn fetch_tables_presto_family(
        &self,
        executor: &dyn SqlExecutor,
        entry: &CatalogEntry,
    ) -> Result<Vec<(String, String, Vec<(String, String)>)>> {
        let schemas = if entry.schemas.is_empty() {
            let sql = match (&self.engine, &entry.catalog) {
                (SqlCatalogEngine::Presto, Some(cat)) => format!("SHOW SCHEMAS FROM {cat}"),
                (SqlCatalogEngine::Presto, None) => "SHOW SCHEMAS".to_string(),
                _ => "SHOW DATABASES".to_string(),
            };
            let rows = executor.execute(&sql).await?;
            let skip = ["information_schema", "pg_catalog", "sys", "system"];
            rows.into_iter()
                .filter_map(|r| r.into_iter().next())
                .filter(|s| !skip.contains(&s.as_str()))
                .collect()
        } else {
            entry.schemas.clone()
        };

        let mut tables = Vec::new();
        for schema in &schemas {
            let sql = match (&self.engine, &entry.catalog) {
                (SqlCatalogEngine::Presto, Some(cat)) => {
                    format!("SHOW TABLES FROM {cat}.{schema}")
                }
                (SqlCatalogEngine::Presto, None) => format!("SHOW TABLES FROM {schema}"),
                _ => format!("SHOW TABLES IN {schema}"),
            };

            let rows = executor.execute(&sql).await.unwrap_or_default();
            for row in &rows {
                let table_name = row.first().cloned().unwrap_or_default();
                if table_name.is_empty() {
                    continue;
                }

                let col_sql = self.columns_sql(entry, schema, &table_name);
                let col_rows = executor.execute(&col_sql).await.unwrap_or_default();
                let columns: Vec<(String, String)> = col_rows
                    .into_iter()
                    .filter_map(|r| {
                        let mut it = r.into_iter();
                        Some((it.next()?, it.next().unwrap_or_default()))
                    })
                    .collect();

                tables.push((schema.clone(), table_name, columns));
            }
        }

        Ok(tables)
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for SqlCatalogAdapter {
    fn name(&self) -> &str {
        &self.skill_name
    }

    fn skill_id(&self) -> &str {
        &self.skill_id
    }

    fn source_family(&self) -> SourceFamily {
        SourceFamily::DbCatalog
    }

    fn labels(&self) -> Vec<String> {
        self.tags.clone()
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if self.entries.is_empty() {
            return Ok(vec![]);
        }

        let query_lower = query.to_lowercase();
        let keywords: Vec<&str> = query_lower.split_whitespace().collect();
        let mut results = Vec::new();

        for entry in &self.entries {
            if results.len() >= limit {
                break;
            }
            match self
                .search_entry(entry, &keywords, limit - results.len())
                .await
            {
                Ok(mut rows) => results.append(&mut rows),
                Err(e) => {
                    tracing::warn!(
                        skill_id = %self.skill_id,
                        label = %entry.label,
                        engine = %self.engine_name(),
                        error = %e,
                        "sql catalog search failed"
                    );
                }
            }
        }

        Ok(results)
    }

    async fn schema_probe(&self, id: &str) -> Result<Vec<serde_json::Value>> {
        // id format: "engine:label:schema.table"
        let (entry, schema, table) = self.parse_table_id(id)?;
        let executor = self.make_executor(entry);
        let col_sql = self.columns_sql(entry, schema, table);
        let rows = executor.execute(&col_sql).await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let name = r.first().cloned().unwrap_or_default();
                let dtype = r.get(1).cloned().unwrap_or_default();
                serde_json::json!({ "name": name, "type": dtype })
            })
            .collect())
    }
}

impl SqlCatalogAdapter {
    fn parse_table_id<'a>(&'a self, id: &'a str) -> Result<(&'a CatalogEntry, &'a str, &'a str)> {
        // id format: "engine:label:schema.table"
        let parts: Vec<&str> = id.splitn(3, ':').collect();
        if parts.len() < 3 {
            return Err(anyhow!("invalid table id: {id}"));
        }
        let label = parts[1];
        let schema_table = parts[2];
        let (schema, table) = schema_table
            .split_once('.')
            .ok_or_else(|| anyhow!("invalid table id: {id}"))?;
        let entry = self
            .entries
            .iter()
            .find(|e| e.label == label)
            .ok_or_else(|| anyhow!("catalog label not found: {label}"))?;
        Ok((entry, schema, table))
    }
}
