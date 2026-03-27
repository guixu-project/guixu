use anyhow::Result;
use data_core::feedback::CommunitySignal;
use data_core::metadata::DatasetMetadata;
use data_core::types::SearchResult;
use serde::Serialize;
use tracing::warn;

use crate::adapters::ExternalAdapter;
use crate::intent::{IntentParser, QueryProfile};
use crate::vector_index::VectorIndex;

/// Search filters that can be applied to results.
#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    pub topic: Option<String>,
    pub min_rows: Option<u64>,
    pub max_price: Option<f64>,
    pub license: Option<String>,
    pub min_quality: Option<f64>,
    pub source: Option<String>,
}

/// Callback to fetch community signal for a dataset CID.
/// Allows the search engine to rank by TCV without owning the feedback store.
pub type SignalFetcher = Box<dyn Fn(&str) -> CommunitySignal + Send + Sync>;

/// The unified search engine. Merges results from local store, DHT,
/// and external adapters (Kaggle, HuggingFace, IPFS, PostgreSQL, DuckDB).
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
    /// `local_metadata` comes from the RocksDB store (P2P-discovered datasets).
    /// `signal_fetcher` retrieves on-chain community feedback for TCV ranking.
    pub async fn search(
        &self,
        query: &str,
        filters: &SearchFilters,
        local_metadata: &[DatasetMetadata],
        signal_fetcher: &SignalFetcher,
        limit: usize,
    ) -> Result<SearchOutput> {
        let profile = self.intent_parser.profile(query).await?;
        self.search_with_profile(&profile, filters, local_metadata, signal_fetcher, limit)
            .await
    }

    /// Search entry point when the caller already has a structured query profile.
    ///
    /// This keeps the existing search behaviour intact while exposing a stable
    /// seam for unit tests and future profiler implementations.
    pub async fn search_with_profile(
        &self,
        profile: &QueryProfile,
        filters: &SearchFilters,
        local_metadata: &[DatasetMetadata],
        signal_fetcher: &SignalFetcher,
        limit: usize,
    ) -> Result<SearchOutput> {
        // Parallel: local P2P store + all external adapters
        let (local_results, (external_results, errors)) = tokio::join!(
            self.search_local(profile, filters, local_metadata),
            self.search_external(profile, filters, limit),
        );

        let mut all: Vec<SearchResult> = local_results.unwrap_or_default();
        all.extend(external_results);

        // Apply filters
        if let Some(min_rows) = filters.min_rows {
            all.retain(|r| r.schema.row_count >= min_rows);
        }
        if let Some(max_price) = filters.max_price {
            all.retain(|r| r.price.amount <= max_price);
        }
        if let Some(ref src) = filters.source {
            all.retain(|r| format!("{:?}", r.source).to_lowercase().contains(&src.to_lowercase()));
        }

        // Deduplicate by CID
        let mut seen = std::collections::HashSet::new();
        all.retain(|r| seen.insert(r.cid.0.clone()));

        // Rank with community signal (TCV-lite for search ranking)
        let mut ranked: Vec<RankedResult> = all
            .into_iter()
            .map(|r| {
                let signal = signal_fetcher(&r.cid.0);
                let score = rank_with_signal(&r, &signal, profile);
                RankedResult { result: r, rank_score: score, signal }
            })
            .collect();

        ranked.sort_by(|a, b| b.rank_score.partial_cmp(&a.rank_score).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(limit);

        Ok(SearchOutput { results: ranked, errors })
    }

    /// Search local P2P metadata store.
    async fn search_local(
        &self,
        intent: &ParsedIntent,
        _filters: &SearchFilters,
        metadata: &[DatasetMetadata],
    ) -> Result<Vec<SearchResult>> {
        let query_lower = intent.raw_query.to_lowercase();
        let keywords = &intent.keywords;

        let results: Vec<SearchResult> = metadata
            .iter()
            .filter(|m| {
                let title = m.title.to_lowercase();
                let desc = m.description.as_deref().unwrap_or("").to_lowercase();
                let tags_str = m.tags.join(" ").to_lowercase();
                let all_text = format!("{title} {desc} {tags_str}");

                // Match if query substring or any keyword matches
                all_text.contains(&query_lower)
                    || keywords.iter().any(|kw| all_text.contains(kw))
            })
            .map(|m| metadata_to_search_result(m))
            .collect();

        Ok(results)
    }

    /// Search external platforms via adapters.
    async fn search_external(
        &self,
        intent: &ParsedIntent,
        _filters: &SearchFilters,
        limit: usize,
    ) -> (Vec<SearchResult>, Vec<String>) {
        let mut results = vec![];
        let mut errors = vec![];
        for adapter in &self.adapters {
            match adapter.search(&intent.raw_query, limit).await {
                Ok(mut r) => results.append(&mut r),
                Err(e) => {
                    warn!(adapter = adapter.name(), error = %e, "adapter search failed");
                    errors.push(format!("{}: {e}", adapter.name()));
                }
            }
        }
        (results, errors)
    }
}

/// A search result annotated with ranking score and community signal.
#[derive(Debug, Clone, Serialize)]
pub struct RankedResult {
    pub result: SearchResult,
    pub rank_score: f64,
    pub signal: CommunitySignal,
}

/// Search output including results and any adapter errors.
#[derive(Debug, Serialize)]
pub struct SearchOutput {
    pub results: Vec<RankedResult>,
    pub errors: Vec<String>,
}

/// Backward-compatible alias for the structured query profile type.
pub type ParsedIntent = QueryProfile;

/// Rank a search result incorporating community signal.
/// This is a lightweight version of TCV used for search ranking.
fn rank_with_signal(result: &SearchResult, signal: &CommunitySignal, intent: &ParsedIntent) -> f64 {
    let quality = result.quality.as_ref().map(|q| q.total).unwrap_or(50.0);

    // Keyword relevance: how many query keywords appear in title/description
    let title_desc = format!(
        "{} {}",
        result.title.to_lowercase(),
        result.description.as_deref().unwrap_or("").to_lowercase()
    );
    let keyword_hits = intent
        .keywords
        .iter()
        .filter(|kw| title_desc.contains(kw.as_str()))
        .count();
    let relevance = if intent.keywords.is_empty() {
        50.0
    } else {
        (keyword_hits as f64 / intent.keywords.len() as f64) * 100.0
    };

    // Community signal: positive reviews boost, negative reviews penalize
    let community = if signal.total_reviews > 0 {
        let base = signal.avg_relevance * 50.0 + 50.0; // map [-1,1] → [0,100]
        let confidence = 1.0 - (1.0 / (1.0 + signal.total_reviews as f64 * 0.2));
        50.0 * (1.0 - confidence) + base * confidence
    } else {
        50.0 // neutral when no reviews
    };

    let risk = signal.risk_penalty();
    let price_penalty = if result.price.is_free() { 0.0 } else { 5.0 };

    // Weighted combination
    0.30 * relevance + 0.25 * quality + 0.25 * community + 0.15 * 70.0 /* freshness placeholder */ - 0.05 * risk - price_penalty
}

/// Convert DatasetMetadata to SearchResult.
fn metadata_to_search_result(m: &DatasetMetadata) -> SearchResult {
    use data_core::types::DataSource;

    SearchResult {
        cid: m.cid.clone(),
        title: m.title.clone(),
        description: m.description.clone(),
        schema: m.schema.clone(),
        quality: None, // computed separately by TCV engine
        price: m.price.clone(),
        license: m.license.clone(),
        provider: m.provider.clone(),
        source: DataSource::P2p,
        data_type: m.data_type.clone(),
        created_at: m.created_at,
    }
}
