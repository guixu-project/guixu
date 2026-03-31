use anyhow::Result;
use data_core::types::*;

use super::ExternalAdapter;

#[derive(Default)]
pub struct DuckDbAdapter {
    pub db_path: Option<String>,
}

#[async_trait::async_trait]
impl ExternalAdapter for DuckDbAdapter {
    fn name(&self) -> &str {
        "duckdb"
    }
    fn source_type(&self) -> DataSource {
        DataSource::DuckDb
    }

    async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SearchResult>> {
        if self.db_path.is_none() {
            return Ok(vec![]);
        }
        // TODO: Real DuckDB catalog query
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// Google Dataset Search — via Google Custom Search JSON API
//   Requires GOOGLE_API_KEY and GOOGLE_CSE_ID env vars.
//   Create a Programmable Search Engine scoped to datasetsearch.research.google.com
//   at https://programmablesearchengine.google.com/
// ---------------------------------------------------------------------------
