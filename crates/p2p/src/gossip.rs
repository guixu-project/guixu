// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::identity::NodeIdentity;
use data_core::metadata::DatasetMetadata;
use tracing::warn;

/// Handle incoming GossipSub messages — verify signature before accepting.
pub fn verify_and_parse(data: &[u8]) -> Result<Option<DatasetMetadata>> {
    let metadata: DatasetMetadata = match serde_json::from_slice(data) {
        Ok(m) => m,
        Err(e) => {
            warn!("gossip: invalid metadata JSON: {e}");
            return Ok(None);
        }
    };

    // Verify provider DID signature
    let pubkey = match NodeIdentity::pubkey_from_did(&metadata.provider) {
        Ok(pk) => pk,
        Err(e) => {
            warn!(did = %metadata.provider.0, "gossip: bad provider DID: {e}");
            return Ok(None);
        }
    };

    let canonical = metadata.canonical_bytes();
    match NodeIdentity::verify(&pubkey, &canonical, &metadata.signature) {
        Ok(()) => Ok(Some(metadata)),
        Err(e) => {
            warn!(cid = %metadata.cid.0, "gossip: signature verification failed: {e}");
            Ok(None)
        }
    }
}
