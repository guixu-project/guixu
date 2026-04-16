// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{bail, Result};
use data_search::engine::{SearchEngine, SearchFilters};
use data_search::intent::{QueryProfile, QueryProfiler};
use data_storage::feedback_store::FeedbackStore;
use data_storage::metadata_store::MetadataStore;
use tokio::task::JoinSet;

use crate::discovery::intent_profiler::{ApiBackedIntentProfiler, HeuristicIntentProfiler};
use crate::discovery::llm::{HostSamplingProvider, LlmProvider};
use crate::discovery::subagent::PlatformSearchSubAgent;
use crate::discovery::types::{DiscoverySearchOutput, DiscoverySearchRequest};
use crate::discovery::workspace::{WorkspaceHandle, WorkspaceManager};
use crate::host_sampling::HostSamplingRuntime;

const DEFAULT_PLATFORMS: &[&str] = &[
    "kaggle",
    "huggingface",
    "google_dataset_search",
    "guixu_hub",
];

pub struct DataDiscoveryRuntime {
    intent_profiler: Arc<dyn QueryProfiler>,
    search_engine: Arc<SearchEngine>,
    feedback_store: FeedbackStore,
    search_workers: usize,
    default_platforms: Vec<String>,
}

impl DataDiscoveryRuntime {
    pub fn try_new(
        search_workers: usize,
        sampling_runtime: Option<Arc<HostSamplingRuntime>>,
        search_engine: Arc<SearchEngine>,
        feedback_store: FeedbackStore,
        _store: MetadataStore,
    ) -> Result<Self> {
        let intent_profiler: Arc<dyn QueryProfiler> = sampling_runtime
            .map(|runtime| {
                let provider: Arc<dyn LlmProvider> = Arc::new(HostSamplingProvider::new(runtime));
                Arc::new(ApiBackedIntentProfiler::new(provider)) as Arc<dyn QueryProfiler>
            })
            .unwrap_or_else(|| Arc::new(HeuristicIntentProfiler));
        Ok(Self {
            intent_profiler,
            search_engine,
            feedback_store,
            search_workers,
            default_platforms: DEFAULT_PLATFORMS
                .iter()
                .map(|platform| (*platform).to_string())
                .collect(),
        })
    }

    pub async fn run_search(
        &self,
        request: DiscoverySearchRequest,
    ) -> Result<DiscoverySearchOutput> {
        let intent = self.profile_query(&request.raw_query).await?;
        let workspace = WorkspaceManager::new(request.raw_query.clone(), intent.clone());
        let platforms = self.select_platforms(&request.filters);
        if platforms.is_empty() {
            bail!("no discovery platforms are configured");
        }

        self.spawn_platform_agents(
            &intent,
            &request.filters,
            request.limit,
            workspace.clone(),
            &platforms,
        )
        .await?;

        let snapshot = workspace.snapshot().await;
        Ok(DiscoverySearchOutput {
            intent,
            results: snapshot.all_results.clone(),
            errors: snapshot.errors.clone(),
            workspace: snapshot,
        })
    }

    async fn profile_query(&self, raw_query: &str) -> Result<QueryProfile> {
        match self.intent_profiler.profile(raw_query).await {
            Ok(profile) => Ok(profile),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "API-backed intent profiling failed; falling back to heuristic profiling"
                );
                HeuristicIntentProfiler.profile(raw_query).await
            }
        }
    }

    fn select_platforms(&self, filters: &SearchFilters) -> Vec<String> {
        let source = if filters.skill_ids.is_empty() {
            self.default_platforms.clone()
        } else {
            filters.skill_ids.clone()
        };

        let mut unique = Vec::new();
        for skill_id in source {
            if skill_id.trim().is_empty() || unique.iter().any(|item: &String| item == &skill_id) {
                continue;
            }
            unique.push(skill_id);
        }
        unique.into_iter().take(self.search_workers).collect()
    }

    async fn spawn_platform_agents(
        &self,
        intent: &QueryProfile,
        filters: &SearchFilters,
        limit: usize,
        workspace: WorkspaceHandle,
        platforms: &[String],
    ) -> Result<()> {
        let mut join_set = JoinSet::new();
        for (index, platform) in platforms.iter().enumerate() {
            let worker_id = format!("worker-{}-{}", index + 1, platform);
            let subagent = PlatformSearchSubAgent {
                agent_id: worker_id.clone(),
                platform_skill_id: platform.clone(),
                search_engine: self.search_engine.clone(),
                workspace: workspace.clone(),
                feedback_store: self.feedback_store.clone(),
            };
            let platform_filters = filters.clone();
            let intent = intent.clone();
            join_set.spawn(async move {
                let result = subagent.run(&intent, &platform_filters, limit).await;
                (worker_id, result)
            });
        }

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok((_worker_id, Ok(()))) => {}
                Ok((worker_id, Err(error))) => {
                    workspace
                        .append_error(&format!("{worker_id}: {error}"))
                        .await?;
                }
                Err(error) => {
                    workspace
                        .append_error(&format!("subagent task join failed: {error}"))
                        .await?;
                }
            }
        }
        Ok(())
    }
}
