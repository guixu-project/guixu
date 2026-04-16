// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Factory functions for common test data structures.

use chrono::Utc;
use data_core::agent::contracts::{
    Budget, DataTaskSpec, DelegatedDataTask, HostContext, HostKind, JobId, TaskPolicy,
    WorkspaceContext,
};
use data_core::feedback::{CommunitySignal, DatasetFeedback, ValueAssessment};
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;

/// Build a minimal `DatasetMetadata` with the given CID.
pub fn metadata(cid: &str) -> DatasetMetadata {
    DatasetMetadata {
        cid: DatasetCid(cid.into()),
        info_hash: None,
        title: format!("Dataset {cid}"),
        description: Some("test dataset".into()),
        tags: vec!["test".into()],
        data_type: DataType::Tabular,
        schema: DatasetSchema {
            columns: vec![],
            row_count: 100,
            size_bytes: 1024,
        },
        stats: None,
        video_meta: None,
        access: AccessMode::Open,
        price: Price::free(),
        license: License {
            spdx_id: "MIT".into(),
            commercial_use: true,
            derivative_allowed: true,
        },
        provider: Did("did:test:provider".into()),
        signature: String::new(),
        provenance: Provenance::Original,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: None,
        previous_version: None,
        verifiable_credential: None,
        source_attributes: None,
    }
}

/// Build a `SearchResult` with the given CID.
pub fn search_result(cid: &str) -> SearchResult {
    SearchResult {
        cid: DatasetCid(cid.into()),
        title: format!("Result {cid}"),
        description: Some("test result".into()),
        tags: vec!["test".into()],
        schema: DatasetSchema {
            columns: vec![],
            row_count: 100,
            size_bytes: 1024,
        },
        quality: None,
        price: Price::free(),
        license: License {
            spdx_id: "MIT".into(),
            commercial_use: true,
            derivative_allowed: true,
        },
        provider: Did("did:test:provider".into()),
        source: DataSource::LocalFile,
        market: None,
        data_type: DataType::Tabular,
        created_at: Utc::now(),
        seller_endpoint: None,
        source_attributes: None,
        provider_meta: None,
        governance: None,
    }
}

/// Build a neutral `CommunitySignal` for the given CID.
pub fn neutral_signal(cid: &str) -> CommunitySignal {
    CommunitySignal {
        dataset_cid: DatasetCid(cid.into()),
        total_reviews: 0,
        avg_relevance: 0.0,
        avg_quality: 0.0,
        positive_rate: 0.0,
        negative_rate: 0.0,
        task_signals: vec![],
    }
}

/// Build a `DatasetFeedback` entry.
pub fn feedback(
    cid: &str,
    relevance: f64,
    quality: u8,
    assessment: ValueAssessment,
) -> DatasetFeedback {
    DatasetFeedback {
        id: uuid::Uuid::new_v4().to_string(),
        dataset_cid: DatasetCid(cid.into()),
        agent_did: Did("did:test:agent".into()),
        task_type: "general".into(),
        task_description: "test task".into(),
        relevance_score: relevance,
        quality_rating: quality,
        task_success: true,
        value_assessment: assessment,
        comment: None,
        timestamp: Utc::now(),
    }
}

/// Build a `DelegatedDataTask` for testing workflow orchestration.
pub fn delegated_task(goal: &str) -> DelegatedDataTask {
    DelegatedDataTask {
        job_id: JobId::new(),
        host: HostContext {
            kind: HostKind::openclaw(),
            session_key: "agent:test:main".into(),
            run_id: None,
        },
        workspace: WorkspaceContext {
            id: "repo:test".into(),
            root_hint: None,
        },
        task: DataTaskSpec {
            goal: goal.into(),
            task_type: Some("evaluation".into()),
            required_modalities: vec![DataType::Tabular],
            required_columns: vec![],
            budget: Some(Budget::usd(10.0)),
        },
        policy: TaskPolicy {
            allow_purchase: false,
            allowed_skill_ids: vec![],
            blocked_skill_ids: vec![],
            allowed_source_families: vec![],
            required_capabilities: vec![SkillCapability::Search],
            require_license_review: false,
        },
        desired_outputs: vec![],
        created_at: Utc::now(),
    }
}

/// Signal fetcher that always returns a neutral signal.
pub fn neutral_signal_fetcher() -> data_search::engine::SignalFetcher {
    Box::new(|cid_str: &str| CommunitySignal {
        dataset_cid: DatasetCid(cid_str.into()),
        total_reviews: 0,
        avg_relevance: 0.0,
        avg_quality: 0.0,
        positive_rate: 0.0,
        negative_rate: 0.0,
        task_signals: vec![],
    })
}
