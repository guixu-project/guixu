// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Contract tests for auth/verifier trust paths.
//!
//! Validates that verify() returns correct TrustLevel based on signature
//! and data integrity status.

use chrono::Utc;
use data_auth::verifier::{verify, TrustLevel};
use data_core::identity::NodeIdentity;
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;
use sha2::{Digest, Sha256};

// ── Helper: build a signed metadata with valid signature ──────────────────────

fn build_signed_metadata(
    identity: &NodeIdentity,
    info_hash: Option<&str>,
    _custom_data: Option<&[u8]>,
) -> DatasetMetadata {
    let cid = DatasetCid("test-cid".into());
    let title = "Test Dataset".into();

    let mut metadata = DatasetMetadata {
        cid: cid.clone(),
        info_hash: info_hash.map(String::from),
        title,
        description: Some("A test dataset".into()),
        tags: vec!["test".into()],
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
        signature: String::new(), // will be set below
        provenance: Provenance::Original,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: None,
        previous_version: None,
        verifiable_credential: None,
        source_attributes: None,
    };

    // Sign the canonical bytes
    let canonical = metadata.canonical_bytes();
    metadata.signature = identity.sign(&canonical);

    metadata
}

// ── Helper: build metadata with wrong signature ─────────────────────────────

fn build_metadata_with_wrong_signature(identity: &NodeIdentity) -> DatasetMetadata {
    let mut metadata = build_signed_metadata(identity, None, None);
    // Overwrite with a wrong signature (64 hex chars = 32 bytes)
    metadata.signature = "deadbeef".repeat(16);
    metadata
}

// ── Helper: build metadata with wrong DID ────────────────────────────────────

fn build_metadata_with_wrong_did() -> DatasetMetadata {
    let identity = NodeIdentity::generate();
    let mut metadata = build_signed_metadata(&identity, None, None);
    // Overwrite provider with a different DID
    metadata.provider = Did("did:key:zQmUnspeakableKey".into());
    metadata
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn correct_signature_with_correct_data_hash_yields_l2() {
    let identity = NodeIdentity::generate();

    // Create data and its matching hash
    let data = b"hello world";
    let hash = hex::encode(Sha256::digest(data));

    let metadata = build_signed_metadata(&identity, Some(&hash), None);

    let report = verify(&metadata, Some(data)).expect("verify should succeed");

    assert!(
        matches!(report.trust_level, TrustLevel::L2SelfClaim),
        "correct sig + correct hash should yield L2, got {:?}",
        report.trust_level
    );
    assert!(report.signature_valid, "signature should be valid");
    assert!(report.integrity_valid, "integrity should be valid");
}

#[test]
fn correct_signature_with_wrong_data_hash_yields_l1() {
    let identity = NodeIdentity::generate();

    // Create data that doesn't match info_hash
    let data = b"hello world";
    let wrong_hash = "abcd".repeat(16); // 64-char hex string

    let metadata = build_signed_metadata(&identity, Some(&wrong_hash), None);

    let report = verify(&metadata, Some(data)).expect("verify should succeed");

    assert!(
        matches!(report.trust_level, TrustLevel::L1Integrity),
        "correct sig + wrong hash should yield L1, got {:?}",
        report.trust_level
    );
    assert!(report.signature_valid, "signature should be valid");
    assert!(!report.integrity_valid, "integrity should be invalid");
}

#[test]
fn correct_signature_without_data_yields_l1() {
    let identity = NodeIdentity::generate();

    // metadata has info_hash but we don't provide data
    let metadata = build_signed_metadata(&identity, Some("abcd".repeat(16).as_str()), None);

    let report = verify(&metadata, None).expect("verify should succeed");

    assert!(
        matches!(report.trust_level, TrustLevel::L1Integrity),
        "correct sig + no data should yield L1, got {:?}",
        report.trust_level
    );
    assert!(report.signature_valid, "signature should be valid");
    assert!(
        !report.integrity_valid,
        "integrity should be invalid when no data provided"
    );
}

#[test]
fn wrong_signature_yields_l0() {
    let identity = NodeIdentity::generate();
    let metadata = build_metadata_with_wrong_signature(&identity);

    let report = verify(&metadata, None).expect("verify should succeed");

    assert!(
        matches!(report.trust_level, TrustLevel::L0Untrusted),
        "wrong signature should yield L0, got {:?}",
        report.trust_level
    );
    assert!(!report.signature_valid, "signature should be invalid");
}

#[test]
fn wrong_did_yields_l0() {
    let metadata = build_metadata_with_wrong_did();

    let report = verify(&metadata, None).expect("verify should succeed");

    assert!(
        matches!(report.trust_level, TrustLevel::L0Untrusted),
        "wrong DID should yield L0, got {:?}",
        report.trust_level
    );
    assert!(!report.signature_valid, "signature should be invalid");
}

#[test]
fn info_hash_none_with_correct_signature_yields_l1() {
    let identity = NodeIdentity::generate();

    // metadata with no info_hash
    let metadata = build_signed_metadata(&identity, None, None);

    let report = verify(&metadata, None).expect("verify should succeed");

    assert!(
        matches!(report.trust_level, TrustLevel::L1Integrity),
        "no info_hash + correct sig should yield L1, got {:?}",
        report.trust_level
    );
    assert!(report.signature_valid, "signature should be valid");
    assert!(
        !report.integrity_valid,
        "integrity should be invalid when info_hash is None"
    );
}

#[test]
fn info_hash_none_with_wrong_signature_yields_l0() {
    let identity = NodeIdentity::generate();
    let mut metadata = build_signed_metadata(&identity, None, None);
    metadata.signature = "bad".repeat(20);

    let report = verify(&metadata, None).expect("verify should succeed");

    assert!(
        matches!(report.trust_level, TrustLevel::L0Untrusted),
        "wrong sig + no info_hash should yield L0, got {:?}",
        report.trust_level
    );
    assert!(!report.signature_valid, "signature should be invalid");
}

#[test]
fn canonical_bytes_excludes_signature() {
    let identity = NodeIdentity::generate();
    let metadata = build_signed_metadata(&identity, None, None);

    // Canonical bytes should be same regardless of signature value
    let mut metadata2 = metadata.clone();
    metadata2.signature = "different".into();

    let canonical1 = metadata.canonical_bytes();
    let canonical2 = metadata2.canonical_bytes();

    assert_eq!(
        canonical1, canonical2,
        "canonical_bytes should exclude signature field"
    );
}

#[test]
fn verification_report_fields_are_consistent() {
    let identity = NodeIdentity::generate();
    let data = b"test data";
    let hash = hex::encode(Sha256::digest(data));

    let metadata = build_signed_metadata(&identity, Some(&hash), None);
    let report = verify(&metadata, Some(data)).expect("verify should succeed");

    // When trust_level is L2, both signature and integrity must be true
    if matches!(report.trust_level, TrustLevel::L2SelfClaim) {
        assert!(report.signature_valid && report.integrity_valid);
    }
    // When trust_level is L1, signature must be true but integrity is false
    if matches!(report.trust_level, TrustLevel::L1Integrity) {
        assert!(report.signature_valid && !report.integrity_valid);
    }
    // When trust_level is L0, at least one of signature/integrity is false
    if matches!(report.trust_level, TrustLevel::L0Untrusted) {
        assert!(!report.signature_valid || !report.integrity_valid);
    }
}
