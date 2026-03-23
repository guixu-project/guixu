use anyhow::Result;
use data_core::types::{DataSource, SearchResult};

use crate::adapters::ExternalAdapter;
use crate::intent::IntentParser;
use crate::vector_index::VectorIndex;

/// Search filters that can be applied to results.
#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    pub topic: Option<String>,
    pub min_rows: Option<u64>,
    pub max_price: Option<f64>,
    pub license: Option<String>,
    pub min_quality: Option<f64>,
}

/// The unified search engine. Merges results from DHT, local vector index,
/// and external adapters (Kaggle, HuggingFace, etc.).
pub struct SearchEngine {
    vector_index: VectorIndex,
    intent_parser: IntentParser,
    adapters: Vec<Box<dyn ExternalAdapter>>,
}

impl SearchEngine {
    pub fn new(
        vector_index: VectorIndex,
        intent_parser: IntentParser,
        adapters: Vec<Box<dyn ExternalAdapter>>,
    ) -> Self {
        Self { vector_index, intent_parser, adapters }
    }

    /// Main search entry point — called by MCP tool `dataset_search`.
    pub async fn search(
        &self,
        query: &str,
        filters: &SearchFilters,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        // 1. Parse intent
        let intent = self.intent_parser.parse(query).await?;

        // 2. Parallel search across all sources
        let (p2p_results, external_results) = tokio::join!(
            self.search_p2p(&intent, filters, limit),
            self.search_external(&intent, filters, limit),
        );

        // 3. Merge, deduplicate by CID, rank
        let mut all = p2p_results.unwrap_or_default();
        all.extend(external_results.unwrap_or_default());
        all.sort_by(|a, b| {
            let sa = self.rank_score(a);
            let sb = self.rank_score(b);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });
        all.truncate(limit);

        Ok(all)
    }

    /// Search P2P network: DHT tag lookup + local vector index.
    async fn search_p2p(
        &self,
        intent: &ParsedIntent,
        filters: &SearchFilters,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        // TODO(milestone-2):
        // 1. DHT tag lookup for each keyword in intent
        // 2. Vector search in local Qdrant for semantic matches
        // 3. Fetch full metadata for matched CIDs
        // 4. Apply filters
        Ok(vec![])
    }

    /// Search external platforms via adapters.
    async fn search_external(
        &self,
        intent: &ParsedIntent,
        filters: &SearchFilters,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let mut results = vec![];
        for adapter in &self.adapters {
            if let Ok(mut r) = adapter.search(&intent.raw_query, limit).await {
                results.append(&mut r);
            }
        }
        Ok(results)
    }

    /// Multi-factor ranking score.
    fn rank_score(&self, result: &SearchResult) -> f64 {
        let quality = result.quality.as_ref().map(|q| q.total).unwrap_or(50.0);
        let freshness = 100.0; // TODO: compute from created_at
        let price_penalty = if result.price.is_free() { 0.0 } else { -10.0 };
        0.4 * quality + 0.3 * freshness + 0.2 * 50.0 /* relevance placeholder */ + price_penalty
    }
}

/// Structured intent parsed from natural language query.
#[derive(Debug, Clone)]
pub struct ParsedIntent {
    pub raw_query: String,
    pub topic: Option<String>,
    pub geo: Option<String>,
    pub temporal: Option<String>,
    pub format: Option<String>,
    pub keywords: Vec<String>,
}
