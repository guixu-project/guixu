// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Stub/mock implementations for external dependencies.

use data_core::types::{SearchResult, SkillCapability, SourceFamily};
use data_search::adapters::ExternalAdapter;
use data_search::engine::SearchEngine;
use data_search::intent::IntentParser;
use data_search::vector_index::VectorIndex;

/// A configurable stub adapter that returns pre-set results.
pub struct StubAdapter {
    pub id: String,
    pub family: SourceFamily,
    pub results: Vec<SearchResult>,
}

impl StubAdapter {
    pub fn new(id: &str, results: Vec<SearchResult>) -> Self {
        Self {
            id: id.into(),
            family: SourceFamily::Custom,
            results,
        }
    }

    pub fn with_family(mut self, family: SourceFamily) -> Self {
        self.family = family;
        self
    }

    pub fn empty(id: &str) -> Self {
        Self::new(id, vec![])
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for StubAdapter {
    fn name(&self) -> &str {
        &self.id
    }

    fn skill_id(&self) -> &str {
        &self.id
    }

    fn source_family(&self) -> SourceFamily {
        self.family
    }

    fn capabilities(&self) -> Vec<SkillCapability> {
        vec![SkillCapability::Search]
    }

    async fn search(&self, _query: &str, _limit: usize) -> anyhow::Result<Vec<SearchResult>> {
        Ok(self.results.clone())
    }
}

/// Build a `SearchEngine` with the given stub adapters.
pub async fn stub_search_engine(adapters: Vec<Box<dyn ExternalAdapter>>) -> SearchEngine {
    SearchEngine::new(
        VectorIndex::init().await.expect("VectorIndex init failed"),
        IntentParser,
        adapters,
    )
}
