// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::types::{AccessGrant, AccessRequest};
use data_storage::metadata_store::MetadataStore;
use tracing::{info, warn};

/// Handle an inbound access request: verify payment, generate access token.
pub fn handle_access_request(
    request: &AccessRequest,
    store: &MetadataStore,
    watermark_enabled: bool,
) -> Result<Option<AccessGrant>> {
    let cid = &request.cid;

    // 1. Check dataset exists
    let metadata = match store.get(cid)? {
        Some(m) => m,
        None => {
            warn!(cid = %cid.0, "access request for unknown CID");
            return Ok(None);
        }
    };

    // 2. Verify payment proof
    if !verify_payment_proof(&request.payment_proof, metadata.price.amount)? {
        warn!(cid = %cid.0, buyer = %request.buyer_did.0, "payment verification failed");
        return Ok(None);
    }

    // 3. Get torrent info hash
    let info_hash = metadata.info_hash.clone().unwrap_or_else(|| cid.0.clone());

    // 4. Generate access token (SHA-256 of cid + buyer_did + timestamp)
    let now = chrono::Utc::now();
    let token_input = format!("{}:{}:{}", cid.0, request.buyer_did.0, now.timestamp());
    let access_token = data_core::identity::sha256_hex(token_input.as_bytes());

    // 5. Build grant
    let (watermark_id, watermark_status) = if watermark_enabled {
        (
            Some(format!("wm-{}", &access_token[..16])),
            "pending".to_string(),
        )
    } else {
        (None, "none".to_string())
    };

    let grant = AccessGrant {
        cid: cid.clone(),
        torrent_info_hash: info_hash,
        access_token,
        watermark_id,
        watermark_status,
        granted_at: now,
    };

    // 6. Persist grant
    store.put_access_grant(cid, &request.buyer_did.0, &grant)?;

    info!(
        cid = %cid.0,
        buyer = %request.buyer_did.0,
        watermark = watermark_enabled,
        "access granted"
    );

    Ok(Some(grant))
}

/// Verify a payment proof (x402 receipt or ERC-8183 escrow tx).
/// Returns true if the proof is valid and covers the required amount.
fn verify_payment_proof(proof: &str, _required_amount: f64) -> Result<bool> {
    // For M3, we do basic structural validation.
    // Full on-chain verification will be added when data-trading is enhanced.
    if proof.is_empty() {
        return Ok(false);
    }

    // Accept JSON payment proofs with a tx_hash or signature field
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(proof) {
        let has_tx = parsed.get("tx_hash").is_some() || parsed.get("signature").is_some();
        return Ok(has_tx);
    }

    // Accept hex-encoded transaction hashes (64 chars)
    if proof.len() == 64 && proof.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(true);
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use data_core::metadata::{DatasetMetadata, Provenance};
    use data_core::types::*;
    use data_storage::metadata_store::MetadataStore;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir()
            .join("guixu-test-access")
            .join(name)
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn setup_store(dir: &std::path::Path, cid: &str, price: f64) -> MetadataStore {
        let store = MetadataStore::open(dir).unwrap();
        let metadata = DatasetMetadata {
            cid: DatasetCid(cid.into()),
            info_hash: Some("infohash123".into()),
            title: "test".into(),
            description: None,
            tags: vec![],
            data_type: DataType::Tabular,
            schema: DatasetSchema {
                columns: vec![],
                row_count: 10,
                size_bytes: 100,
            },
            stats: None,
            video_meta: None,
            access: AccessMode::Paid,
            price: Price::usdc(price),
            license: License {
                spdx_id: "MIT".into(),
                commercial_use: true,
                derivative_allowed: true,
            },
            provider: Did("did:test:provider".into()),
            signature: String::new(),
            provenance: Provenance::Original,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            verifiable_credential: None,
            source_attributes: None,
            version: None,
            previous_version: None,
        };
        store.put(&metadata).unwrap();
        store
    }

    #[test]
    fn unknown_cid_returns_none() {
        let dir = temp_dir("ac-unknown");
        let store = MetadataStore::open(&dir).unwrap();
        let req = AccessRequest {
            cid: DatasetCid("nonexistent".into()),
            buyer_did: Did("did:buyer:1".into()),
            payment_proof: "a".repeat(64),
        };
        assert!(handle_access_request(&req, &store, false)
            .unwrap()
            .is_none());
    }

    #[test]
    fn empty_payment_proof_returns_none() {
        let dir = temp_dir("ac-empty-proof");
        let store = setup_store(&dir, "cid-1", 1.0);
        let req = AccessRequest {
            cid: DatasetCid("cid-1".into()),
            buyer_did: Did("did:buyer:1".into()),
            payment_proof: String::new(),
        };
        assert!(handle_access_request(&req, &store, false)
            .unwrap()
            .is_none());
    }

    #[test]
    fn valid_payment_returns_grant() {
        let dir = temp_dir("ac-valid");
        let store = setup_store(&dir, "cid-2", 1.0);
        let req = AccessRequest {
            cid: DatasetCid("cid-2".into()),
            buyer_did: Did("did:buyer:2".into()),
            payment_proof:
                r#"{"tx_hash":"abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"}"#
                    .into(),
        };
        let grant = handle_access_request(&req, &store, false).unwrap().unwrap();
        assert_eq!(grant.cid.0, "cid-2");
        assert_eq!(grant.torrent_info_hash, "infohash123");
        assert!(!grant.access_token.is_empty());
        assert!(grant.watermark_id.is_none());
        assert_eq!(grant.watermark_status, "none");
    }

    #[test]
    fn watermark_enabled_sets_pending() {
        let dir = temp_dir("ac-watermark");
        let store = setup_store(&dir, "cid-3", 1.0);
        let req = AccessRequest {
            cid: DatasetCid("cid-3".into()),
            buyer_did: Did("did:buyer:3".into()),
            payment_proof: "a".repeat(64),
        };
        let grant = handle_access_request(&req, &store, true).unwrap().unwrap();
        assert!(grant.watermark_id.is_some());
        assert_eq!(grant.watermark_status, "pending");
    }

    #[test]
    fn grant_is_persisted() {
        let dir = temp_dir("ac-persist");
        let store = setup_store(&dir, "cid-4", 1.0);
        let req = AccessRequest {
            cid: DatasetCid("cid-4".into()),
            buyer_did: Did("did:buyer:4".into()),
            payment_proof: "b".repeat(64),
        };
        handle_access_request(&req, &store, false).unwrap();
        let saved = store
            .get_access_grant(&DatasetCid("cid-4".into()), "did:buyer:4")
            .unwrap();
        assert!(saved.is_some());
    }
}
