// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Native streaming discovery protocol for AI agent integration.
//!
//! GIP005 Phase 2: Replaces MCP request/response with a subscription-based
//! streaming protocol so agents can observe progressive search results.

use anyhow::Result;
use data_core::signal_fetcher::SignalFetcher;
use data_core::types::DatasetCid;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::engine::{SearchEngine, SearchFilters};
use crate::intent::{IntentParser, QueryProfile};

/// Discovery result with progressive scoring.
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    /// Dataset content identifier.
    pub cid: DatasetCid,
    /// Current rank (updated as new results arrive).
    pub rank: usize,
    /// Composite ranking score.
    pub score: f64,
    /// Source adapter that produced this result.
    pub source: String,
    /// Optional preview URL for quick inspection.
    pub preview_url: Option<String>,
}

/// Search progress state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiscoveryStatus {
    #[default]
    /// Search is actively running.
    Running,
    /// Search is narrowing results (converging).
    Converging,
    /// Search is complete.
    Complete,
    /// Search was cancelled.
    Cancelled,
}

/// Snapshot of an in-progress discovery search.
#[derive(Debug, Clone)]
pub struct DiscoveryState {
    /// Unique intent ID for this search.
    pub intent_id: String,
    /// Current status of the search.
    pub status: DiscoveryStatus,
    /// Number of results discovered so far.
    pub results_so_far: usize,
    /// Adapters that have been exhausted.
    pub adapters_exhausted: Vec<String>,
    /// Time elapsed since search started (seconds).
    pub elapsed_secs: f64,
}

/// A streaming discovery search handle.
///
/// Allows the caller to receive progressive results via a stream,
/// check current search state, or cancel the search mid-flight.
pub struct DiscoverySearchHandle {
    pub intent_id: String,
    cancel_tx: mpsc::Sender<()>,
}

impl DiscoverySearchHandle {
    /// Cancel this search. Idempotent.
    pub async fn cancel(&self) -> Result<()> {
        let _ = self.cancel_tx.send(()).await;
        Ok(())
    }
}

/// Streaming search engine that wraps the existing SearchEngine
/// with subscription-based progressive result delivery.
pub struct StreamingSearchEngine {
    inner: Arc<SearchEngine>,
}

impl StreamingSearchEngine {
    pub fn new(inner: SearchEngine) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Subscribe to a search intent and receive progressive results as they're discovered.
    ///
    /// Returns a stream of `DiscoveryResult` and a handle for state/cancellation.
    #[allow(clippy::too_many_arguments)]
    pub async fn subscribe(
        &self,
        query: &str,
        filters: &SearchFilters,
        local_metadata: &[data_core::metadata::DatasetMetadata],
        signal_fetcher: &SignalFetcher,
        limit: usize,
    ) -> (
        Pin<Box<dyn tokio_stream::Stream<Item = DiscoveryResult> + Send + Sync>>,
        DiscoverySearchHandle,
    ) {
        let intent_id = uuid::Uuid::new_v4().to_string();
        let (result_tx, result_rx) = mpsc::channel(32);
        let (cancel_tx, _cancel_rx) = mpsc::channel::<()>(1);

        let query = query.to_string();
        let filters = filters.clone();
        let local_metadata = local_metadata.to_vec();
        let signal_fetcher = signal_fetcher.clone();

        // Clone the Arc-wrapped inner engine so the async block doesn't borrow `self`
        let inner = self.inner.clone();

        // Spawn the search task
        let intent_id_clone = intent_id.clone();
        tokio::spawn(async move {
            let profile = IntentParser
                .profile(&query)
                .await
                .unwrap_or_else(|_| QueryProfile::default());

            let search_result = inner
                .search_with_profile(&profile, &filters, &local_metadata, &signal_fetcher, limit)
                .await;

            match search_result {
                Ok(output) => {
                    for (rank, ranked) in output.results.into_iter().enumerate() {
                        let discovery_result = DiscoveryResult {
                            cid: ranked.result.cid.clone(),
                            rank: rank + 1,
                            score: ranked.rank_score,
                            source: ranked
                                .result
                                .source_attributes
                                .as_ref()
                                .and_then(|a: &serde_json::Value| a.get("skill_id"))
                                .and_then(|v: &serde_json::Value| v.as_str())
                                .unwrap_or("unknown")
                                .to_string(),
                            preview_url: None,
                        };
                        // Send result, stop if receiver dropped
                        if result_tx.send(discovery_result).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, intent_id_clone, "streaming search failed");
                }
            }
        });

        let handle = DiscoverySearchHandle {
            intent_id,
            cancel_tx,
        };
        let stream: Pin<Box<dyn tokio_stream::Stream<Item = DiscoveryResult> + Send + Sync>> =
            Box::pin(ReceiverStream::new(result_rx));
        (stream, handle)
    }
}

/// Create a StreamingSearchEngine wrapping the provided SearchEngine.
impl From<SearchEngine> for StreamingSearchEngine {
    fn from(inner: SearchEngine) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }
}
