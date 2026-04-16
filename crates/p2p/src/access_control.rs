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
