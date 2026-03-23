use anyhow::Result;
use data_core::types::SearchResult;

/// Trait for external dataset platform adapters.
#[async_trait::async_trait]
pub trait ExternalAdapter: Send + Sync {
    fn name(&self) -> &str;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;
}

/// Kaggle API adapter.
pub struct KaggleAdapter {
    // TODO(milestone-3): Kaggle API token
}

#[async_trait::async_trait]
impl ExternalAdapter for KaggleAdapter {
    fn name(&self) -> &str { "kaggle" }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // TODO(milestone-3): Call Kaggle API → map to SearchResult
        Ok(vec![])
    }
}

/// HuggingFace Hub adapter.
pub struct HuggingFaceAdapter;

#[async_trait::async_trait]
impl ExternalAdapter for HuggingFaceAdapter {
    fn name(&self) -> &str { "huggingface" }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // TODO(milestone-3): Call HF API → map to SearchResult
        Ok(vec![])
    }
}
