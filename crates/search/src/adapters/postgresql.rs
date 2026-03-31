use anyhow::Result;
use data_core::types::*;

use super::ExternalAdapter;

#[derive(Default)]
pub struct PostgreSqlAdapter {
    pub connection_string: Option<String>,
}

#[async_trait::async_trait]
impl ExternalAdapter for PostgreSqlAdapter {
    fn name(&self) -> &str {
        "postgresql"
    }
    fn source_type(&self) -> DataSource {
        DataSource::PostgreSql
    }

    async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SearchResult>> {
        if self.connection_string.is_none() {
            return Ok(vec![]);
        }
        // TODO: Real PostgreSQL information_schema query
        Ok(vec![])
    }
}
