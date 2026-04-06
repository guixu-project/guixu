// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

pub mod contracts;
pub mod workflow;

pub fn workflow_state_from_app_state(
    store: data_storage::metadata_store::MetadataStore,
    feedback_store: data_storage::feedback_store::FeedbackStore,
    search_engine: data_search::engine::SearchEngine,
) -> workflow::WorkflowState {
    workflow::WorkflowState::new(store, feedback_store, search_engine)
}
