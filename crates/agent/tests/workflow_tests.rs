// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Tests for the workflow orchestration service.

use std::sync::Arc;

use data_core::agent::contracts::JobState;
use data_core::agent::memory::MemoryKey;
use data_core::types::DataSource;
use data_search::engine::SearchEngine;
use data_test_utils::fixtures;
use data_test_utils::stores::TempStores;
use data_test_utils::stubs::StubAdapter;

use data_agent::workflow::{WorkflowService, WorkflowState};

fn build_state(
    stores: &TempStores,
    adapters: Vec<Box<dyn data_search::adapters::ExternalAdapter>>,
) -> WorkflowState {
    let engine = SearchEngine::new(
        data_search::vector_index::VectorIndex,
        data_search::intent::IntentParser,
        adapters,
    );
    WorkflowState::new(
        stores.metadata.clone(),
        stores.feedback.clone(),
        Arc::new(engine),
        stores.job.clone(),
        stores.memory.clone(),
    )
}

fn make_result(cid: &str, source: DataSource) -> data_core::types::SearchResult {
    let mut r = fixtures::search_result(cid);
    r.source = source;
    r
}

// ---------------------------------------------------------------------------
// WorkflowState
// ---------------------------------------------------------------------------

#[test]
fn signal_fetcher_returns_neutral_for_unknown_cid() {
    let stores = TempStores::new();
    let state = build_state(&stores, vec![]);
    let fetcher = state.signal_fetcher();
    let signal = fetcher("nonexistent-cid");
    assert_eq!(signal.total_reviews, 0);
    assert_eq!(signal.avg_relevance, 0.0);
}

// ---------------------------------------------------------------------------
// WorkflowService::run — success path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_selects_best_candidate_and_completes_job() {
    let stores = TempStores::new();
    let results = vec![make_result("cid-best", DataSource::LocalFile)];
    let adapter = StubAdapter::new("test_skill", results);
    let state = build_state(&stores, vec![Box::new(adapter)]);
    let svc = WorkflowService::new(state);

    let mut task = fixtures::delegated_task("find tabular data");
    task.policy.allowed_skill_ids = vec!["test_skill".into()];
    let job_id = task.job_id.clone();

    let result = svc.run(task).await.unwrap();
    assert!(result.errors.is_empty());
    assert_eq!(result.selected_dataset.unwrap().0, "cid-best");

    // Job state should be Completed
    let status = stores.job.get_status(&job_id).unwrap().unwrap();
    assert_eq!(status.state, JobState::Completed);

    // Memory should have been written
    assert!(!result.memory_updates.is_empty());
}

// ---------------------------------------------------------------------------
// WorkflowService::run — no candidates
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_records_failure_when_no_candidates() {
    let stores = TempStores::new();
    let adapter = StubAdapter::empty("empty_skill");
    let state = build_state(&stores, vec![Box::new(adapter)]);
    let svc = WorkflowService::new(state);

    let mut task = fixtures::delegated_task("find nonexistent data");
    task.policy.allowed_skill_ids = vec!["empty_skill".into()];
    let job_id = task.job_id.clone();

    let result = svc.run(task).await.unwrap();
    // run() returns Ok even on failure — errors are in the result
    assert!(result.selected_dataset.is_none());
    assert!(!result.errors.is_empty());
    assert!(result.errors[0].contains("no candidates"));

    let status = stores.job.get_status(&job_id).unwrap().unwrap();
    assert_eq!(status.state, JobState::Failed);
}

// ---------------------------------------------------------------------------
// Memory fallback chain
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_writes_memory_with_correct_key() {
    let stores = TempStores::new();
    let results = vec![make_result("cid-mem", DataSource::LocalFile)];
    let adapter = StubAdapter::new("mem_skill", results);
    let state = build_state(&stores, vec![Box::new(adapter)]);
    let svc = WorkflowService::new(state);

    let mut task = fixtures::delegated_task("memory test");
    task.policy.allowed_skill_ids = vec!["mem_skill".into()];
    task.task.task_type = Some("classification".into());

    let result = svc.run(task.clone()).await.unwrap();
    assert!(result.selected_dataset.is_some());

    // Memory should be stored under the task family key
    let key = MemoryKey::task_family(
        &task.workspace.id,
        task.host.kind.clone(),
        task.task.task_type.as_deref().unwrap(),
    );
    let memory = stores.memory.get(&key).unwrap();
    assert!(memory.is_some());
    let memory = memory.unwrap();
    assert!(!memory.successful_mappings.is_empty());
}

// ---------------------------------------------------------------------------
// Multiple adapters — parallel search
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_merges_results_from_multiple_adapters() {
    let stores = TempStores::new();
    let a1 = StubAdapter::new("skill_a", vec![make_result("cid-a", DataSource::Kaggle)]);
    let a2 = StubAdapter::new(
        "skill_b",
        vec![make_result("cid-b", DataSource::HuggingFace)],
    );
    let state = build_state(&stores, vec![Box::new(a1), Box::new(a2)]);
    let svc = WorkflowService::new(state);

    let mut task = fixtures::delegated_task("multi adapter test");
    task.policy.allowed_skill_ids = vec!["skill_a".into(), "skill_b".into()];

    let result = svc.run(task).await.unwrap();
    assert!(result.selected_dataset.is_some());
    // Should have completed successfully with at least one candidate
    assert!(result.errors.is_empty());
}

// ---------------------------------------------------------------------------
// Job events are recorded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_records_job_events() {
    let stores = TempStores::new();
    let adapter = StubAdapter::new(
        "ev_skill",
        vec![make_result("cid-ev", DataSource::LocalFile)],
    );
    let state = build_state(&stores, vec![Box::new(adapter)]);
    let svc = WorkflowService::new(state);

    let mut task = fixtures::delegated_task("event test");
    task.policy.allowed_skill_ids = vec!["ev_skill".into()];
    let job_id = task.job_id.clone();

    svc.run(task).await.unwrap();

    let events = stores.job.list_events(&job_id).unwrap();
    // Should have at least: JobStarted, plan (JobQueued), worker events, JobCompleted
    assert!(events.len() >= 3);
}

// ---------------------------------------------------------------------------
// create_signal_fetcher standalone function
// ---------------------------------------------------------------------------

#[test]
fn create_signal_fetcher_returns_neutral_for_missing_cid() {
    let stores = TempStores::new();
    let fetcher = data_agent::workflow::create_signal_fetcher(&stores.feedback);
    let signal = fetcher("missing-cid");
    assert_eq!(signal.total_reviews, 0);
}
