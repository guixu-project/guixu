// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::discovery::types::{ObservationRecord, WorkerProgress, WorkerRegistration};
use crate::discovery::workspace::WorkspaceHandle;
use anyhow::Result;
use data_core::feedback::CommunitySignal;
use data_core::types::DatasetCid;
use data_search::engine::{RankedResult, SearchEngine, SearchFilters};
use data_search::intent::QueryProfile;
use data_storage::feedback_store::FeedbackStore;

pub struct PlatformSearchSubAgent {
    pub agent_id: String,
    pub platform_skill_id: String,
    pub search_engine: Arc<SearchEngine>,
    pub workspace: WorkspaceHandle,
    pub feedback_store: FeedbackStore,
}

impl PlatformSearchSubAgent {
    pub async fn run(
        &self,
        intent: &QueryProfile,
        _base_filters: &SearchFilters,
        limit: usize,
    ) -> Result<()> {
        self.workspace
            .register_worker(WorkerRegistration {
                worker_id: self.agent_id.clone(),
                skill_id: self.platform_skill_id.clone(),
            })
            .await?;

        match self.run_inner(intent, limit).await {
            Ok(progress) => {
                self.workspace
                    .update_worker_progress(&self.agent_id, progress)
                    .await?;
                self.workspace.complete_worker(&self.agent_id).await?;
            }
            Err(error) => {
                self.workspace
                    .fail_worker(&self.agent_id, &error.to_string())
                    .await?;
            }
        }
        Ok(())
    }

    async fn run_inner(&self, intent: &QueryProfile, limit: usize) -> Result<WorkerProgress> {
        let query = build_platform_query(intent);
        let batch = self.search_one_query(&query, limit).await?;

        let total_results = batch.len();
        for result in batch {
            self.workspace
                .append_observation(ObservationRecord {
                    worker_id: self.agent_id.clone(),
                    skill_id: self.platform_skill_id.clone(),
                    query_variant: query.clone(),
                    result,
                })
                .await?;
        }

        Ok(WorkerProgress {
            turn_index: 1,
            query_count: 1,
            result_count: total_results,
        })
    }

    async fn search_one_query(&self, query: &str, limit: usize) -> Result<Vec<RankedResult>> {
        let feedback_store = self.feedback_store.clone();
        let signal_fetcher: data_search::engine::SignalFetcher = Box::new(move |cid_str: &str| {
            let cid = DatasetCid(cid_str.to_string());
            feedback_store
                .compute_signal(&cid)
                .unwrap_or_else(|_| CommunitySignal {
                    dataset_cid: cid,
                    total_reviews: 0,
                    avg_relevance: 0.0,
                    avg_quality: 0.0,
                    positive_rate: 0.0,
                    negative_rate: 0.0,
                    task_signals: vec![],
                })
        });

        self.search_engine
            .search_single_skill_raw(&self.platform_skill_id, query, &signal_fetcher, limit)
            .await
    }
}

fn build_platform_query(intent: &QueryProfile) -> String {
    let query = if intent.keywords.is_empty() {
        intent.raw_query.trim().to_string()
    } else {
        intent.keywords.join(" ")
    };

    if query.is_empty() {
        intent.raw_query.clone()
    } else {
        query
    }
}
