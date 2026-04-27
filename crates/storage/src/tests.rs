// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use chrono::Utc;
use data_core::feedback::{DatasetFeedback, ValueAssessment};
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;

use crate::feedback_store::FeedbackStore;
use crate::metadata_store::MetadataStore;

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir()
        .join("guixu-test")
        .join(name)
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn make_metadata(cid: &str) -> DatasetMetadata {
    DatasetMetadata {
        cid: DatasetCid(cid.into()),
        info_hash: None,
        title: format!("Dataset {cid}"),
        description: Some("test".into()),
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
        price: Price::usdc(1.0),
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
        verifiable_credential: None,
        source_attributes: None,
        version: None,
        previous_version: None,
    }
}

// --- MetadataStore tests ---

#[test]
fn metadata_put_and_get() {
    let dir = temp_dir("meta-put-get");
    let store = MetadataStore::open(&dir).unwrap();
    let metadata = make_metadata("cid-1");
    store.put(&metadata).unwrap();

    let got = store.get(&DatasetCid("cid-1".into())).unwrap();
    assert!(got.is_some());
    assert_eq!(got.unwrap().title, "Dataset cid-1");
}

#[test]
fn metadata_get_missing_returns_none() {
    let dir = temp_dir("meta-missing");
    let store = MetadataStore::open(&dir).unwrap();
    let got = store.get(&DatasetCid("nonexistent".into())).unwrap();
    assert!(got.is_none());
}

#[test]
fn metadata_list_all() {
    let dir = temp_dir("meta-list");
    let store = MetadataStore::open(&dir).unwrap();
    store.put(&make_metadata("a")).unwrap();
    store.put(&make_metadata("b")).unwrap();

    let all = store.list_all().unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn metadata_file_path_roundtrip() {
    let dir = temp_dir("meta-filepath");
    let store = MetadataStore::open(&dir).unwrap();
    let cid = DatasetCid("cid-fp".into());
    let path = std::path::PathBuf::from("/tmp/data.csv");

    store.put_file_path(&cid, &path).unwrap();
    let got = store.get_file_path(&cid).unwrap();
    assert_eq!(got.unwrap(), path);
}

// --- FeedbackStore tests ---

fn make_feedback(cid: &str, id: &str, positive: bool) -> DatasetFeedback {
    DatasetFeedback {
        id: id.into(),
        dataset_cid: DatasetCid(cid.into()),
        agent_did: Did("did:test:agent".into()),
        task_type: "classification".into(),
        task_description: "test task".into(),
        relevance_score: if positive { 0.9 } else { 0.2 },
        quality_rating: if positive { 5 } else { 1 },
        value_assessment: if positive {
            ValueAssessment::Positive
        } else {
            ValueAssessment::Negative
        },
        task_success: positive,
        comment: None,
        timestamp: Utc::now(),
    }
}

#[test]
fn feedback_put_and_get() {
    let dir = temp_dir("fb-put-get");
    let store = FeedbackStore::open(&dir).unwrap();
    let feedback = make_feedback("cid-1", "fb-1", true);
    store.put(&feedback).unwrap();

    let feedbacks = store.get_for_dataset(&DatasetCid("cid-1".into())).unwrap();
    assert_eq!(feedbacks.len(), 1);
    assert_eq!(feedbacks[0].id, "fb-1");
}

#[test]
fn feedback_compute_signal_empty() {
    let dir = temp_dir("fb-signal-empty");
    let store = FeedbackStore::open(&dir).unwrap();
    let signal = store
        .compute_signal(&DatasetCid("cid-none".into()))
        .unwrap();
    assert_eq!(signal.total_reviews, 0);
}

#[test]
fn feedback_compute_signal_mixed() {
    let dir = temp_dir("fb-signal-mixed");
    let store = FeedbackStore::open(&dir).unwrap();
    store.put(&make_feedback("cid-1", "fb-1", true)).unwrap();
    store.put(&make_feedback("cid-1", "fb-2", false)).unwrap();

    let signal = store.compute_signal(&DatasetCid("cid-1".into())).unwrap();
    assert_eq!(signal.total_reviews, 2);
    assert!((signal.positive_rate - 0.5).abs() < f64::EPSILON);
    assert!((signal.negative_rate - 0.5).abs() < f64::EPSILON);
}

#[test]
fn sync_state_put_and_get() {
    let dir = temp_dir("sync-state");
    let store = MetadataStore::open(&dir).unwrap();

    assert!(store.get_sync_state("defillama").unwrap().is_none());
    store.put_sync_state("defillama", 1700000000).unwrap();
    assert_eq!(store.get_sync_state("defillama").unwrap(), Some(1700000000));
    // Overwrite
    store.put_sync_state("defillama", 1700001000).unwrap();
    assert_eq!(store.get_sync_state("defillama").unwrap(), Some(1700001000));
    // Different source
    assert!(store.get_sync_state("rwa_xyz").unwrap().is_none());
}

// --- SeedRecord CRUD tests ---

#[test]
fn seed_put_get_delete() {
    let dir = temp_dir("seed-crud");
    let store = MetadataStore::open(&dir).unwrap();
    let seed = SeedRecord {
        info_hash: "abc123".into(),
        cid: DatasetCid("cid-s1".into()),
        file_path: "/tmp/data.csv".into(),
        access: AccessMode::Open,
        title: "test seed".into(),
        size_bytes: 2048,
        created_at: Utc::now(),
    };
    store.put_seed(&seed).unwrap();

    let got = store.get_seed("abc123").unwrap().unwrap();
    assert_eq!(got.cid.0, "cid-s1");
    assert_eq!(got.size_bytes, 2048);

    store.delete_seed("abc123").unwrap();
    assert!(store.get_seed("abc123").unwrap().is_none());
}

#[test]
fn seed_list_returns_all() {
    let dir = temp_dir("seed-list");
    let store = MetadataStore::open(&dir).unwrap();
    for i in 0..3 {
        store
            .put_seed(&SeedRecord {
                info_hash: format!("hash{i}"),
                cid: DatasetCid(format!("cid{i}")),
                file_path: format!("/tmp/{i}.csv").into(),
                access: AccessMode::Open,
                title: format!("seed {i}"),
                size_bytes: 100,
                created_at: Utc::now(),
            })
            .unwrap();
    }
    assert_eq!(store.list_seeds().unwrap().len(), 3);
}

#[test]
fn seed_get_missing_returns_none() {
    let dir = temp_dir("seed-missing");
    let store = MetadataStore::open(&dir).unwrap();
    assert!(store.get_seed("nonexistent").unwrap().is_none());
}

// --- AccessGrant CRUD tests ---

#[test]
fn access_grant_put_and_get() {
    let dir = temp_dir("access-grant");
    let store = MetadataStore::open(&dir).unwrap();
    let cid = DatasetCid("cid-ag".into());
    let grant = AccessGrant {
        cid: cid.clone(),
        torrent_info_hash: "hash123".into(),
        access_token: "token456".into(),
        watermark_id: None,
        watermark_status: "none".into(),
        granted_at: Utc::now(),
    };
    store.put_access_grant(&cid, "did:buyer:1", &grant).unwrap();

    let got = store
        .get_access_grant(&cid, "did:buyer:1")
        .unwrap()
        .unwrap();
    assert_eq!(got.access_token, "token456");
    assert_eq!(got.torrent_info_hash, "hash123");
}

#[test]
fn access_grant_missing_returns_none() {
    let dir = temp_dir("access-grant-miss");
    let store = MetadataStore::open(&dir).unwrap();
    let cid = DatasetCid("cid-none".into());
    assert!(store.get_access_grant(&cid, "did:x").unwrap().is_none());
}

// --- mark_unpublished tests ---

#[test]
fn mark_unpublished_removes_from_cache() {
    let dir = temp_dir("unpublish");
    let store = MetadataStore::open(&dir).unwrap();
    let m = make_metadata("cid-up");
    store.put(&m).unwrap();
    assert!(store.get(&DatasetCid("cid-up".into())).unwrap().is_some());

    store
        .mark_unpublished(&DatasetCid("cid-up".into()))
        .unwrap();
    assert!(store.is_unpublished(&DatasetCid("cid-up".into())).unwrap());
    // Should be removed from list_all cache
    let all = store.list_all().unwrap();
    assert!(all.iter().all(|m| m.cid.0 != "cid-up"));
}

// --- Provider reputation tests ---

#[test]
fn provider_reputation_no_feedback_returns_neutral() {
    let dir = temp_dir("prov-rep-empty");
    let store = FeedbackStore::open(&dir).unwrap();
    let (score, reviews, _) = store.compute_provider_reputation("did:test:p", &[]);
    assert_eq!(score, 50.0);
    assert_eq!(reviews, 0);
}

#[test]
fn provider_reputation_positive_feedback_scores_high() {
    let dir = temp_dir("prov-rep-pos");
    let store = FeedbackStore::open(&dir).unwrap();
    let cid = DatasetCid("cid-rep".into());
    store.put(&make_feedback("cid-rep", "f1", true)).unwrap();
    store.put(&make_feedback("cid-rep", "f2", true)).unwrap();

    let (score, reviews, avg_q) = store.compute_provider_reputation("did:test:p", &[cid]);
    assert_eq!(reviews, 2);
    assert!(
        score > 50.0,
        "positive feedback should score above neutral: {score}"
    );
    assert!(avg_q > 3.0);
}

#[test]
fn provider_reputation_negative_feedback_scores_low() {
    let dir = temp_dir("prov-rep-neg");
    let store = FeedbackStore::open(&dir).unwrap();
    let cid = DatasetCid("cid-neg".into());
    store.put(&make_feedback("cid-neg", "f1", false)).unwrap();
    store.put(&make_feedback("cid-neg", "f2", false)).unwrap();

    let (score, reviews, _) = store.compute_provider_reputation("did:test:p", &[cid]);
    assert_eq!(reviews, 2);
    assert!(
        score < 50.0,
        "negative feedback should score below neutral: {score}"
    );
}

#[test]
fn provider_reputation_aggregates_across_datasets() {
    let dir = temp_dir("prov-rep-multi");
    let store = FeedbackStore::open(&dir).unwrap();
    store.put(&make_feedback("cid-a", "f1", true)).unwrap();
    store.put(&make_feedback("cid-b", "f2", true)).unwrap();
    store.put(&make_feedback("cid-b", "f3", false)).unwrap();

    let cids = vec![DatasetCid("cid-a".into()), DatasetCid("cid-b".into())];
    let (_, reviews, _) = store.compute_provider_reputation("did:test:p", &cids);
    assert_eq!(reviews, 3);
}
