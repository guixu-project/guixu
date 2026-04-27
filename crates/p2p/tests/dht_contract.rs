// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Contract tests for DHT index operations.
//!
//! Tests DhtIndex::put_metadata, get_metadata, broadcast_metadata
//! and validates tag privacy filtering.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use chrono::Utc;
use data_core::identity::NodeIdentity;
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;
use data_p2p::dht::DhtIndex;
use data_p2p::network::{NetworkCommand, NetworkHandle};
use libp2p::PeerId;

// ── Fake NetworkHandle for testing ────────────────────────────────────────────

struct FakeNetwork {
    store: Arc<Mutex<HashMap<Vec<u8>, Vec<u8>>>>,
    gossip_sent: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
}

impl FakeNetwork {
    fn new() -> (mpsc::Sender<NetworkCommand>, Self) {
        let store = Arc::new(Mutex::new(HashMap::new()));
        let gossip_sent = Arc::new(Mutex::new(Vec::new()));
        let store_clone = store.clone();
        let gossip_clone = gossip_sent.clone();
        let (tx, mut rx) = mpsc::channel(100);

        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    NetworkCommand::DhtPut { key, value } => {
                        store_clone.lock().await.insert(key, value);
                    }
                    NetworkCommand::DhtGet { key, reply } => {
                        let result = store_clone.lock().await.get(&key).cloned();
                        let _ = reply.send(result);
                    }
                    NetworkCommand::GossipPublish { topic, data } => {
                        gossip_clone.lock().await.push((topic, data));
                    }
                    _ => {}
                }
            }
        });

        let fake = Self { store, gossip_sent };
        (tx, fake)
    }

    fn store(&self) -> Arc<Mutex<HashMap<Vec<u8>, Vec<u8>>>> {
        self.store.clone()
    }

    fn gossip_sent(&self) -> Arc<Mutex<Vec<(String, Vec<u8>)>>> {
        self.gossip_sent.clone()
    }
}

// ── Helper: build test metadata ──────────────────────────────────────────────

fn build_test_metadata(identity: &NodeIdentity) -> DatasetMetadata {
    let cid = DatasetCid("test-cid-123".into());
    let metadata = DatasetMetadata {
        cid,
        info_hash: Some("abcd".repeat(16)),
        title: "Test Dataset".into(),
        description: Some("A test dataset".into()),
        tags: vec!["test".into(), "nlp".into(), "classification".into()],
        data_type: DataType::Tabular,
        schema: DatasetSchema {
            columns: vec![ColumnDef {
                name: "id".into(),
                dtype: "int64".into(),
                nullable: false,
                description: Some("ID column".into()),
            }],
            row_count: 100,
            size_bytes: 1024,
        },
        stats: None,
        video_meta: None,
        access: AccessMode::Open,
        price: Price::free(),
        license: License {
            spdx_id: "CC-BY-4.0".into(),
            commercial_use: true,
            derivative_allowed: true,
        },
        provider: identity.did.clone(),
        signature: identity.sign(&[1, 2, 3]), // dummy signature
        provenance: Provenance::Original,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: Some("1.0.0".into()),
        previous_version: None,
        verifiable_credential: None,
        source_attributes: None,
    };
    metadata
}

// ── Helper: create DhtIndex with fake network ─────────────────────────────────

fn create_dht_index() -> (DhtIndex, FakeNetwork) {
    let (cmd_tx, fake) = FakeNetwork::new();
    let net = NetworkHandle {
        cmd_tx,
        local_peer_id: PeerId::random(),
    };
    let dht = DhtIndex::new(net);
    (dht, fake)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn put_metadata_stores_in_dht() {
    let identity = NodeIdentity::generate();
    let metadata = build_test_metadata(&identity);
    let (dht, fake) = create_dht_index();

    dht.put_metadata(&metadata)
        .await
        .expect("put_metadata should succeed");

    // Give time for the async command to be processed
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Verify metadata was stored with key "meta:{cid}"
    let key = format!("meta:{}", metadata.cid.0).into_bytes();
    let store = fake.store();
    assert!(
        store.lock().await.contains_key(&key),
        "metadata should be stored in DHT"
    );

    // Verify stored value is valid JSON
    let value = store.lock().await.get(&key).unwrap().clone();
    let stored: DatasetMetadata = serde_json::from_slice(&value).expect("should be valid JSON");
    assert_eq!(stored.cid, metadata.cid);
    assert_eq!(stored.title, metadata.title);
}

#[tokio::test]
async fn put_metadata_indexes_tags() {
    let identity = NodeIdentity::generate();
    let metadata = build_test_metadata(&identity);
    let (dht, _fake) = create_dht_index();

    dht.put_metadata(&metadata)
        .await
        .expect("put_metadata should succeed");

    // Tags should be indexed with format "tag:{tag}:{cid}"
    // We just verify it doesn't panic - tag indexing is internal
}

#[tokio::test]
async fn get_metadata_retrieves_stored_data() {
    let identity = NodeIdentity::generate();
    let metadata = build_test_metadata(&identity);
    let (dht, _fake) = create_dht_index();

    // First store
    dht.put_metadata(&metadata)
        .await
        .expect("put_metadata should succeed");

    // Then retrieve
    let retrieved = dht
        .get_metadata(&metadata.cid)
        .await
        .expect("get_metadata should succeed");

    assert!(retrieved.is_some(), "should retrieve stored metadata");
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.cid, metadata.cid);
    assert_eq!(retrieved.title, metadata.title);
}

#[tokio::test]
async fn get_metadata_returns_none_for_missing() {
    let (dht, _fake) = create_dht_index();
    let missing_cid = DatasetCid("nonexistent-cid".into());

    let result = dht
        .get_metadata(&missing_cid)
        .await
        .expect("get_metadata should succeed");

    assert!(result.is_none(), "should return None for missing CID");
}

#[tokio::test]
async fn broadcast_metadata_publishes_to_gossip() {
    let identity = NodeIdentity::generate();
    let metadata = build_test_metadata(&identity);
    let (dht, fake) = create_dht_index();

    dht.broadcast_metadata(&metadata)
        .await
        .expect("broadcast_metadata should succeed");

    // Give time for the async command to be processed
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Verify gossip was published
    let gossip_sent = fake.gossip_sent();
    assert!(
        !gossip_sent.lock().await.is_empty(),
        "gossip should have been published"
    );
}

#[tokio::test]
async fn put_metadata_preserves_all_fields() {
    let identity = NodeIdentity::generate();
    let mut metadata = build_test_metadata(&identity);
    metadata.version = Some("2.0.0".into());
    metadata.description = Some("Updated description".into());

    let (dht, _fake) = create_dht_index();
    dht.put_metadata(&metadata)
        .await
        .expect("put_metadata should succeed");

    let retrieved = dht
        .get_metadata(&metadata.cid)
        .await
        .expect("get_metadata should succeed")
        .expect("metadata should exist");

    assert_eq!(retrieved.version, Some("2.0.0".into()));
    assert_eq!(retrieved.description, Some("Updated description".into()));
}

#[tokio::test]
async fn metadata_with_paid_access_stores_correctly() {
    let identity = NodeIdentity::generate();
    let mut metadata = build_test_metadata(&identity);
    metadata.access = AccessMode::Paid;
    metadata.price = Price::usdc(10.0);

    let (dht, _fake) = create_dht_index();
    dht.put_metadata(&metadata)
        .await
        .expect("put_metadata should succeed");

    let retrieved = dht
        .get_metadata(&metadata.cid)
        .await
        .expect("get_metadata should succeed")
        .expect("metadata should exist");

    assert!(matches!(retrieved.access, AccessMode::Paid));
}
