// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use data_core::agent::contracts::{
    DelegatedDataTask, JobEvent, JobEventType, JobId, JobResult, JobState, WorkerTask,
    WorkerTaskKind,
};
use data_core::feedback::CommunitySignal;
use data_core::types::{DataSource, DatasetCid};
use data_search::engine::{SearchEngine, SignalFetcher};
use data_search::intent::QueryProfile;
use data_storage::feedback_store::FeedbackStore;
use data_storage::job_store::JobStore;
use data_storage::metadata_store::MetadataStore;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::task::JoinSet;

#[derive(Clone)]
pub struct WorkflowState {
    pub store: Arc<MetadataStore>,
    pub feedback_store: Arc<FeedbackStore>,
    pub search_engine: Arc<SearchEngine>,
    pub job_store: Arc<JobStore>,
}

impl WorkflowState {
    pub fn new(
        store: MetadataStore,
        feedback_store: FeedbackStore,
        search_engine: SearchEngine,
        job_store: JobStore,
    ) -> Self {
        Self {
            store: Arc::new(store),
            feedback_store: Arc::new(feedback_store),
            search_engine: Arc::new(search_engine),
            job_store: Arc::new(job_store),
        }
    }

    pub fn signal_fetcher(&self) -> SignalFetcher {
        let fb_store = self.feedback_store.clone();
        Box::new(move |cid_str: &str| {
            let cid = DatasetCid(cid_str.to_string());
            fb_store
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
        })
    }
}

pub struct WorkflowService {
    state: WorkflowState,
}

impl WorkflowService {
    pub fn new(state: WorkflowState) -> Self {
        Self { state }
    }

    pub async fn run(&self, task: DelegatedDataTask) -> anyhow::Result<JobResult> {
        let job_id = task.job_id.clone();

        tracing::info!(job_id = %job_id, goal = %task.task.goal, "starting workflow");

        self.state
            .job_store
            .create_job(job_id.clone(), JobState::Running)?;
        let _ = self.state.job_store.append_event(&JobEvent::new(
            job_id.clone(),
            JobEventType::JobStarted,
            format!("workflow started for goal: {}", task.task.goal),
            None,
            json!({ "goal": task.task.goal }),
        ));

        let result = self.execute_workflow(task).await;

        match result {
            Ok(mut job_result) => {
                job_result.job_id = job_id.clone();
                let final_state = if job_result.errors.is_empty() {
                    JobState::Completed
                } else {
                    JobState::Failed
                };
                let _ = self.state.job_store.update_state(&job_id, final_state);
                let _ = self.state.job_store.put_result(&job_result);
                let _ = self.state.job_store.append_event(&JobEvent::new(
                    job_id.clone(),
                    if final_state == JobState::Completed {
                        JobEventType::JobCompleted
                    } else {
                        JobEventType::JobFailed
                    },
                    "workflow finished",
                    None,
                    json!({
                        "selected_dataset": job_result.selected_dataset.as_ref().map(|cid| cid.0.clone()),
                        "errors": job_result.errors,
                    }),
                ));
                Ok(job_result)
            }
            Err(e) => {
                tracing::error!(job_id = %job_id, error = %e, "workflow failed");
                let _ = self.state.job_store.update_state(&job_id, JobState::Failed);
                let job_result = JobResult {
                    job_id,
                    selected_dataset: None,
                    artifacts: vec![],
                    memory_updates: vec![],
                    errors: vec![e.to_string()],
                };
                let _ = self.state.job_store.put_result(&job_result);
                let _ = self.state.job_store.append_event(&JobEvent::new(
                    job_id,
                    JobEventType::JobFailed,
                    "workflow failed",
                    None,
                    json!({ "error": e.to_string() }),
                ));
                Ok(job_result)
            }
        }
    }

    async fn execute_workflow(&self, task: DelegatedDataTask) -> anyhow::Result<JobResult> {
        let search_output = self.search(&task).await?;
        let selected = self.evaluate_and_select(search_output, &task).await?;

        Ok(JobResult {
            job_id: JobId::new(),
            selected_dataset: Some(selected),
            artifacts: vec![],
            memory_updates: vec![],
            errors: vec![],
        })
    }

    async fn search(&self, task: &DelegatedDataTask) -> anyhow::Result<Value> {
        let query = &task.task.goal;
        let base_filters = data_search::engine::SearchFilters {
            topic: None,
            min_rows: None,
            max_price: task.task.budget.as_ref().map(|b| b.amount),
            license: None,
            min_quality: None,
            source: None,
            chain: None,
            protocol: None,
            asset: None,
            category: None,
            free_only: Some(!task.policy.allow_purchase),
        };

        let local_metadata = self.state.store.list_all()?;
        let query_profile = QueryProfile {
            raw_query: query.clone(),
            task_type: task.task.task_type.clone(),
            task_description: Some(query.clone()),
            target_entity: None,
            keywords: query.split_whitespace().map(String::from).collect(),
            data_standard: Default::default(),
            user_profile: Default::default(),
        };

        let sources = selected_sources(&task.policy.allowed_sources);
        let mut join_set = JoinSet::new();

        for source in sources {
            let worker = WorkerTask::new(
                task.job_id.clone(),
                WorkerTaskKind::SearchSource,
                format!("search source {}", source_filter_name(&source)),
            );
            let _ = self.state.job_store.append_event(&JobEvent::new(
                task.job_id.clone(),
                JobEventType::WorkerStarted,
                format!("worker started: {}", worker.label),
                Some(worker.task_id.clone()),
                json!({
                    "worker_kind": worker.kind,
                    "label": worker.label,
                    "source": source_filter_name(&source),
                }),
            ));

            let state = self.state.clone();
            let local_metadata = local_metadata.clone();
            let profile = query_profile.clone();
            let job_id = task.job_id.clone();
            let worker_id = worker.task_id.clone();
            let mut filters = base_filters.clone();
            filters.source = Some(source_filter_name(&source).to_string());

            join_set.spawn(async move {
                let signal_fetcher = state.signal_fetcher();
                let output = state
                    .search_engine
                    .search_with_profile(&profile, &filters, &local_metadata, &signal_fetcher, 10)
                    .await;
                (job_id, worker_id, source, output)
            });
        }

        let mut merged_results = Vec::new();
        while let Some(joined) = join_set.join_next().await {
            let (job_id, worker_id, source, output) = joined?;
            match output {
                Ok(search_output) => {
                    let candidate_count = search_output.results.len();
                    let _ = self.state.job_store.append_event(&JobEvent::new(
                        job_id,
                        JobEventType::WorkerCompleted,
                        "search worker completed",
                        Some(worker_id),
                        json!({
                            "source": source_filter_name(&source),
                            "candidate_count": candidate_count,
                        }),
                    ));
                    merged_results.extend(search_output.results);
                }
                Err(error) => {
                    let _ = self.state.job_store.append_event(&JobEvent::new(
                        job_id,
                        JobEventType::WorkerFailed,
                        format!("search worker failed for {}", source_filter_name(&source)),
                        Some(worker_id),
                        json!({
                            "source": source_filter_name(&source),
                            "error": error.to_string(),
                        }),
                    ));
                }
            }
        }

        let results: Vec<Value> = merged_results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                json!({
                    "rank": i + 1,
                    "cid": r.result.cid.0,
                    "title": r.result.title,
                    "description": r.result.description,
                    "source": r.result.source,
                    "data_type": r.result.data_type,
                    "price": r.result.price,
                    "rank_score": r.rank_score,
                })
            })
            .collect();

        Ok(json!({ "candidates": results }))
    }

    async fn evaluate_and_select(
        &self,
        search_result: Value,
        task: &DelegatedDataTask,
    ) -> anyhow::Result<DatasetCid> {
        let worker = WorkerTask::new(
            task.job_id.clone(),
            WorkerTaskKind::EvaluateCandidate,
            "evaluate and select best candidate",
        );
        let _ = self.state.job_store.append_event(&JobEvent::new(
            task.job_id.clone(),
            JobEventType::WorkerStarted,
            format!("worker started: {}", worker.label),
            Some(worker.task_id.clone()),
            json!({ "worker_kind": worker.kind, "label": worker.label }),
        ));

        let candidates = search_result
            .get("candidates")
            .and_then(|v| v.as_array())
            .map(|arr| arr.to_vec())
            .unwrap_or_default();

        if candidates.is_empty() {
            let _ = self.state.job_store.append_event(&JobEvent::new(
                task.job_id.clone(),
                JobEventType::WorkerFailed,
                "candidate evaluation failed: no candidates found",
                Some(worker.task_id),
                json!({ "reason": "no_candidates" }),
            ));
            anyhow::bail!("no candidates found");
        }

        let best = candidates.first().unwrap();
        let cid_str = best
            .get("cid")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let _ = self.state.job_store.append_event(&JobEvent::new(
            task.job_id.clone(),
            JobEventType::WorkerCompleted,
            "candidate evaluation completed",
            Some(worker.task_id),
            json!({ "selected_dataset": cid_str }),
        ));

        Ok(DatasetCid(cid_str.to_string()))
    }
}

fn selected_sources(allowed_sources: &[DataSource]) -> Vec<DataSource> {
    if allowed_sources.is_empty() {
        vec![
            DataSource::Kaggle,
            DataSource::HuggingFace,
            DataSource::Ipfs,
        ]
    } else {
        allowed_sources.to_vec()
    }
}

fn source_filter_name(source: &DataSource) -> &'static str {
    match source {
        DataSource::Kaggle => "kaggle",
        DataSource::HuggingFace => "huggingface",
        DataSource::Ipfs => "ipfs",
        DataSource::BitTorrent => "bittorrent",
        DataSource::GuixuHub => "guixu-hub",
        DataSource::DefiLlama => "defillama",
        DataSource::RwaXyz => "rwa_xyz",
        DataSource::Arxiv => "arxiv",
        DataSource::Dblp => "dblp",
        DataSource::SemanticScholar => "semanticscholar",
        DataSource::PanSearch => "pansearch",
        DataSource::P2p => "p2p",
        DataSource::PostgreSql => "postgresql",
        DataSource::DuckDb => "duckdb",
        DataSource::LocalFile => "localfile",
        DataSource::GoogleDatasetSearch => "googledatasetsearch",
        DataSource::DataCiteCommons => "datacitecommons",
        DataSource::TheGraph => "thegraph",
        DataSource::Spark => "spark",
        DataSource::Flink => "flink",
        DataSource::Presto => "presto",
        DataSource::OpenDataSkill => "opendataskill",
    }
}

pub fn create_signal_fetcher(
    feedback_store: &FeedbackStore,
) -> impl Fn(&str) -> CommunitySignal + 'static {
    let fb_store = feedback_store.clone();
    move |cid_str: &str| {
        let cid = DatasetCid(cid_str.to_string());
        fb_store
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
    }
}
