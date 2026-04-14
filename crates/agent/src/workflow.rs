// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use data_core::agent::contracts::{
    DelegatedDataTask, JobEvent, JobEventType, JobId, JobResult, JobState, WorkerTask,
    WorkerTaskKind,
};
use data_core::agent::memory::{AgentMemory, MemoryKey};
use data_core::feedback::CommunitySignal;
use data_core::types::{DatasetCid, SkillCapability};
use data_search::engine::{RankedResult, SearchEngine, SignalFetcher};
use data_search::intent::QueryProfile;
use data_storage::feedback_store::FeedbackStore;
use data_storage::job_store::JobStore;
use data_storage::memory_store::MemoryStore;
use data_storage::metadata_store::MetadataStore;
use serde_json::json;
use tokio::task::JoinSet;

use data_storage::trace_manager::{AgentTraceManager, SpanBuilder, SpanType};

use crate::planner::{
    ExecutionStrategy, PlannedOperation, PlannedSkillTask, Planner, SkillExecutionPlan,
    StopConditionKind,
};

// ---------------------------------------------------------------------------
// WorkflowState
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct WorkflowState {
    pub store: Arc<MetadataStore>,
    pub feedback_store: Arc<FeedbackStore>,
    pub search_engine: Arc<SearchEngine>,
    pub job_store: Arc<JobStore>,
    pub memory_store: Arc<MemoryStore>,
}

impl WorkflowState {
    pub fn new(
        store: MetadataStore,
        feedback_store: FeedbackStore,
        search_engine: Arc<SearchEngine>,
        job_store: JobStore,
        memory_store: MemoryStore,
    ) -> Self {
        Self {
            store: Arc::new(store),
            feedback_store: Arc::new(feedback_store),
            search_engine,
            job_store: Arc::new(job_store),
            memory_store: Arc::new(memory_store),
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

// ---------------------------------------------------------------------------
// Memory key derivation
// ---------------------------------------------------------------------------

fn memory_key_for_task(task: &DelegatedDataTask) -> MemoryKey {
    match &task.task.task_type {
        Some(task_type) => MemoryKey::task_family(&task.workspace.id, task.host.kind, task_type),
        None => MemoryKey::workspace(&task.workspace.id, task.host.kind),
    }
}

/// Load memory with fallback: TaskFamily → Workspace → Global → empty.
fn load_memory_with_fallback(store: &MemoryStore, task: &DelegatedDataTask) -> AgentMemory {
    let primary = memory_key_for_task(task);
    if let Ok(Some(mem)) = store.get(&primary) {
        return mem;
    }
    let workspace = MemoryKey::workspace(&task.workspace.id, task.host.kind);
    if let Ok(Some(mem)) = store.get(&workspace) {
        return mem;
    }
    let global = MemoryKey::global(task.host.kind);
    if let Ok(Some(mem)) = store.get(&global) {
        return mem;
    }
    AgentMemory {
        scope: primary.to_scope(),
        key: primary,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// WorkflowService
// ---------------------------------------------------------------------------

pub struct WorkflowService {
    state: WorkflowState,
    trace_manager: Option<Arc<RwLock<AgentTraceManager>>>,
}

impl WorkflowService {
    pub fn new(state: WorkflowState) -> Self {
        Self {
            state,
            trace_manager: None,
        }
    }

    pub fn with_trace_manager(mut self, tm: Arc<RwLock<AgentTraceManager>>) -> Self {
        self.trace_manager = Some(tm);
        self
    }

    pub async fn run(&self, task: DelegatedDataTask) -> anyhow::Result<JobResult> {
        let job_id = task.job_id.clone();

        // Start trace span if tracing is enabled
        let (trace_id, span_id) = if let Some(tm) = &self.trace_manager {
            let tm = tm.read().await;
            tm.start_trace("workflow.run", SpanType::Agent)
                .await
                .unwrap_or_else(|| (String::new(), String::new()))
        } else {
            (String::new(), String::new())
        };
        let trace_id_clone = trace_id.clone();
        let span_id_clone = span_id.clone();

        // 1. Load memory
        let mut memory = load_memory_with_fallback(&self.state.memory_store, &task);
        let memory_key = memory_key_for_task(&task);

        // 2. Plan (memory-aware)
        let plan = Planner::build(&task, Some(&memory));

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
        let _ = self.record_plan(&plan);

        // 3. Execute
        let result = self.execute_workflow(&task, &plan).await;

        // End trace span
        if let (Some(tm), true) = (&self.trace_manager, !trace_id_clone.is_empty()) {
            let tm = tm.write().await;
            let builder = SpanBuilder::new(
                &trace_id_clone,
                &span_id_clone,
                None,
                "workflow.run",
                SpanType::Agent,
            )
            .with_attribute("job_id", serde_json::json!(job_id.to_string()))
            .with_attribute("goal", serde_json::json!(task.task.goal));
            let builder = if result.is_err() {
                builder.with_error(result.as_ref().err().unwrap().to_string().as_str())
            } else {
                builder
            };
            tm.end_span(builder).await;
        }

        // 4. Write memory + finalize
        match result {
            Ok((selected_cid, winning_skill, rank_score)) => {
                memory.record_successful_mapping(
                    &task.task.goal,
                    &selected_cid.0,
                    &winning_skill,
                    rank_score,
                );
                memory.key = memory_key.clone();
                memory.scope = memory_key.to_scope();
                let _ = self.state.memory_store.put(&memory);

                let memory_key_str = memory_key.to_storage_key();
                let job_result = JobResult {
                    job_id: job_id.clone(),
                    selected_dataset: Some(selected_cid),
                    artifacts: vec![],
                    memory_updates: vec![memory_key_str],
                    errors: vec![],
                };
                let _ = self
                    .state
                    .job_store
                    .update_state(&job_id, JobState::Completed);
                let _ = self.state.job_store.put_result(&job_result);
                let _ = self.state.job_store.append_event(&JobEvent::new(
                    job_id,
                    JobEventType::JobCompleted,
                    "workflow finished",
                    None,
                    json!({
                        "selected_dataset": job_result.selected_dataset.as_ref().map(|c| &c.0),
                    }),
                ));
                Ok(job_result)
            }
            Err(e) => {
                tracing::error!(job_id = %job_id, error = %e, "workflow failed");
                memory.record_failed_attempt(&job_id.0, &task.task.goal, &e.to_string());
                memory.key = memory_key.clone();
                memory.scope = memory_key.to_scope();
                let _ = self.state.memory_store.put(&memory);

                let memory_key_str = memory_key.to_storage_key();
                let _ = self.state.job_store.update_state(&job_id, JobState::Failed);
                let job_result = JobResult {
                    job_id: job_id.clone(),
                    selected_dataset: None,
                    artifacts: vec![],
                    memory_updates: vec![memory_key_str],
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

    /// Returns (selected_cid, winning_skill_id, rank_score).
    async fn execute_workflow(
        &self,
        task: &DelegatedDataTask,
        plan: &SkillExecutionPlan,
    ) -> anyhow::Result<(DatasetCid, String, f64)> {
        let merged = self.search(plan, task).await?;
        self.evaluate_and_select(merged, task)
    }

    fn record_plan(&self, plan: &SkillExecutionPlan) -> anyhow::Result<()> {
        self.state.job_store.append_event(&JobEvent::new(
            plan.job_id.clone(),
            JobEventType::JobQueued,
            "planner generated execution plan",
            None,
            json!({
                "stages": plan.stages.iter().map(|stage| json!({
                    "stage_id": stage.stage_id,
                    "name": stage.name,
                    "strategy": stage.strategy,
                    "tasks": stage.tasks.iter().map(|t| json!({
                        "task_id": t.task_id,
                        "skill_id": t.skill_id,
                        "source_family": t.source_family,
                        "operation": t.operation,
                        "priority": t.priority,
                        "timeout_ms": t.timeout_ms,
                    })).collect::<Vec<_>>()
                })).collect::<Vec<_>>(),
                "stop_conditions": plan.stop_conditions,
                "budget_policy": plan.budget_policy,
                "rationale": plan.rationale,
            }),
        ))?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Search — respects ExecutionStrategy, timeout, and StopCondition
    // -----------------------------------------------------------------------

    async fn search(
        &self,
        plan: &SkillExecutionPlan,
        task: &DelegatedDataTask,
    ) -> anyhow::Result<Vec<RankedResult>> {
        let enough = plan
            .stop_conditions
            .iter()
            .find(|c| c.kind == StopConditionKind::EnoughCandidates)
            .and_then(|c| c.threshold)
            .unwrap_or(20) as usize;

        let mut all_results = Vec::new();

        for stage in &plan.stages {
            let search_tasks: Vec<_> = stage
                .tasks
                .iter()
                .filter(|t| t.operation == PlannedOperation::Search)
                .cloned()
                .collect();
            if search_tasks.is_empty() {
                continue;
            }

            let mut stage_results = match stage.strategy {
                ExecutionStrategy::Parallel => {
                    self.search_parallel(
                        &search_tasks,
                        task,
                        enough.saturating_sub(all_results.len()),
                    )
                    .await
                }
                ExecutionStrategy::Sequential => {
                    self.search_sequential(
                        &search_tasks,
                        task,
                        enough.saturating_sub(all_results.len()),
                    )
                    .await
                }
                ExecutionStrategy::Fallback => {
                    self.search_fallback(
                        &search_tasks,
                        task,
                        enough.saturating_sub(all_results.len()),
                    )
                    .await
                }
            };

            all_results.append(&mut stage_results);
            if all_results.len() >= enough {
                break;
            }
        }

        Ok(all_results)
    }

    async fn search_parallel(
        &self,
        tasks: &[PlannedSkillTask],
        task: &DelegatedDataTask,
        limit: usize,
    ) -> Vec<RankedResult> {
        let (tx, mut rx) =
            tokio::sync::mpsc::channel::<(String, String, PlannedSkillTask, Vec<RankedResult>)>(32);
        let mut join_set = JoinSet::new();

        for planned_task in tasks {
            let worker = WorkerTask::new(
                task.job_id.clone(),
                WorkerTaskKind::SearchSkill,
                format!("search skill {}", planned_task.skill_id),
            );
            self.emit_worker_started(task, &worker, planned_task);

            let state = self.state.clone();
            let search_ctx = self.build_search_context(task, planned_task);
            let timeout_dur = Duration::from_millis(planned_task.timeout_ms);
            let tx = tx.clone();
            let job_id = task.job_id.clone();
            let worker_id = worker.task_id.clone();
            let pt = planned_task.clone();

            join_set.spawn(async move {
                let result = tokio::time::timeout(timeout_dur, async {
                    let (profile, filters, local_metadata) = search_ctx;
                    let signal_fetcher = state.signal_fetcher();
                    state
                        .search_engine
                        .search_with_profile(
                            &profile,
                            &filters,
                            &local_metadata,
                            &signal_fetcher,
                            10,
                        )
                        .await
                })
                .await;

                match result {
                    Ok(Ok(output)) => {
                        let _ = tx.send((job_id.0, worker_id, pt, output.results)).await;
                    }
                    Ok(Err(_)) | Err(_) => {
                        let _ = tx.send((job_id.0, worker_id, pt, vec![])).await;
                    }
                }
            });
        }
        drop(tx);

        let mut results = Vec::new();
        while let Some((job_id_str, worker_id, planned_task, batch)) = rx.recv().await {
            let job_id = JobId(job_id_str);
            let count = batch.len();
            let event_type = if count > 0 {
                JobEventType::WorkerCompleted
            } else {
                JobEventType::WorkerFailed
            };
            let _ = self.state.job_store.append_event(&JobEvent::new(
                job_id,
                event_type,
                format!(
                    "search worker {} — {} candidates",
                    planned_task.skill_id, count
                ),
                Some(worker_id),
                json!({ "skill_id": planned_task.skill_id, "candidate_count": count }),
            ));
            results.extend(batch);
            if results.len() >= limit {
                join_set.abort_all();
                // drain remaining
                while rx.recv().await.is_some() {}
                break;
            }
        }
        results
    }

    async fn search_sequential(
        &self,
        tasks: &[PlannedSkillTask],
        task: &DelegatedDataTask,
        limit: usize,
    ) -> Vec<RankedResult> {
        let mut results = Vec::new();
        for planned_task in tasks {
            if results.len() >= limit {
                break;
            }
            let batch = self.search_one(task, planned_task).await;
            results.extend(batch);
        }
        results
    }

    async fn search_fallback(
        &self,
        tasks: &[PlannedSkillTask],
        task: &DelegatedDataTask,
        _limit: usize,
    ) -> Vec<RankedResult> {
        let mut sorted = tasks.to_vec();
        sorted.sort_by_key(|t| t.priority);
        for planned_task in &sorted {
            let batch = self.search_one(task, planned_task).await;
            if !batch.is_empty() {
                return batch;
            }
        }
        vec![]
    }

    async fn search_one(
        &self,
        task: &DelegatedDataTask,
        planned_task: &PlannedSkillTask,
    ) -> Vec<RankedResult> {
        let worker = WorkerTask::new(
            task.job_id.clone(),
            WorkerTaskKind::SearchSkill,
            format!("search skill {}", planned_task.skill_id),
        );
        self.emit_worker_started(task, &worker, planned_task);

        let (profile, filters, local_metadata) = self.build_search_context(task, planned_task);
        let signal_fetcher = self.state.signal_fetcher();
        let timeout_dur = Duration::from_millis(planned_task.timeout_ms);

        let result = tokio::time::timeout(
            timeout_dur,
            self.state.search_engine.search_with_profile(
                &profile,
                &filters,
                &local_metadata,
                &signal_fetcher,
                10,
            ),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let count = output.results.len();
                let _ = self.state.job_store.append_event(&JobEvent::new(
                    task.job_id.clone(),
                    JobEventType::WorkerCompleted,
                    format!(
                        "search worker {} — {} candidates",
                        planned_task.skill_id, count
                    ),
                    Some(worker.task_id),
                    json!({ "skill_id": planned_task.skill_id, "candidate_count": count }),
                ));
                output.results
            }
            Ok(Err(e)) => {
                let _ = self.state.job_store.append_event(&JobEvent::new(
                    task.job_id.clone(),
                    JobEventType::WorkerFailed,
                    format!("search worker failed: {}", planned_task.skill_id),
                    Some(worker.task_id),
                    json!({ "skill_id": planned_task.skill_id, "error": e.to_string() }),
                ));
                vec![]
            }
            Err(_) => {
                let _ = self.state.job_store.append_event(&JobEvent::new(
                    task.job_id.clone(),
                    JobEventType::WorkerFailed,
                    format!("search worker timed out: {}", planned_task.skill_id),
                    Some(worker.task_id),
                    json!({ "skill_id": planned_task.skill_id, "error": "timeout" }),
                ));
                vec![]
            }
        }
    }

    fn build_search_context(
        &self,
        task: &DelegatedDataTask,
        planned_task: &PlannedSkillTask,
    ) -> (
        QueryProfile,
        data_search::engine::SearchFilters,
        Vec<data_core::metadata::DatasetMetadata>,
    ) {
        let query = &task.task.goal;
        let mut filters = data_search::engine::SearchFilters {
            topic: None,
            min_rows: None,
            max_price: task.task.budget.as_ref().map(|b| b.amount),
            license: None,
            min_quality: None,
            skill_ids: vec![planned_task.skill_id.clone()],
            source_families: planned_task.source_family.into_iter().collect(),
            required_capabilities: task.policy.required_capabilities.clone(),
            chain: None,
            protocol: None,
            asset: None,
            category: None,
            free_only: Some(!task.policy.allow_purchase),
        };
        if filters.required_capabilities.is_empty() {
            filters.required_capabilities = vec![SkillCapability::Search];
        }

        let profile = QueryProfile {
            raw_query: query.clone(),
            task_type: task.task.task_type.clone(),
            task_description: Some(query.clone()),
            target_entity: None,
            keywords: query.split_whitespace().map(String::from).collect(),
            data_standard: Default::default(),
            user_profile: Default::default(),
        };

        let local_metadata = self.state.store.list_all().unwrap_or_default();
        (profile, filters, local_metadata)
    }

    fn emit_worker_started(
        &self,
        task: &DelegatedDataTask,
        worker: &WorkerTask,
        planned_task: &PlannedSkillTask,
    ) {
        let _ = self.state.job_store.append_event(&JobEvent::new(
            task.job_id.clone(),
            JobEventType::WorkerStarted,
            format!("worker started: {}", worker.label),
            Some(worker.task_id.clone()),
            json!({
                "worker_kind": worker.kind,
                "label": worker.label,
                "skill_id": planned_task.skill_id,
                "source_family": planned_task.source_family,
            }),
        ));
    }

    // -----------------------------------------------------------------------
    // Evaluate
    // -----------------------------------------------------------------------

    fn evaluate_and_select(
        &self,
        results: Vec<RankedResult>,
        task: &DelegatedDataTask,
    ) -> anyhow::Result<(DatasetCid, String, f64)> {
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

        if results.is_empty() {
            let _ = self.state.job_store.append_event(&JobEvent::new(
                task.job_id.clone(),
                JobEventType::WorkerFailed,
                "no candidates found",
                Some(worker.task_id),
                json!({ "reason": "no_candidates" }),
            ));
            anyhow::bail!("no candidates found");
        }

        let best = &results[0];
        let cid = best.result.cid.clone();
        let skill_id = best
            .result
            .provider_meta
            .as_ref()
            .map(|m| m.provider_id.clone())
            .or_else(|| {
                // Extract skill_id from CID prefix "skill:<id>:..."
                best.result
                    .cid
                    .0
                    .strip_prefix("skill:")
                    .and_then(|rest| rest.split_once(':').map(|(id, _)| id.to_string()))
            })
            .unwrap_or_else(|| format!("{:?}", best.result.source).to_lowercase());
        let rank_score = best.rank_score;

        let _ = self.state.job_store.append_event(&JobEvent::new(
            task.job_id.clone(),
            JobEventType::WorkerCompleted,
            "candidate evaluation completed",
            Some(worker.task_id),
            json!({
                "selected_dataset": cid.0,
                "skill_id": skill_id,
                "rank_score": rank_score,
            }),
        ));

        Ok((cid, skill_id, rank_score))
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
