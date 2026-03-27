use anyhow::Result;
use data_core::identity::NodeIdentity;
use data_core::metadata::DatasetMetadata;
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub struct VerificationReport {
    pub signature_valid: bool,
    pub integrity_valid: bool,
    pub trust_level: TrustLevel,
}

#[derive(Debug, Clone, Copy)]
pub enum TrustLevel {
    L0Untrusted,
    L1Integrity,
    L2SelfClaim,
}

/// Verify dataset metadata signature and optional data integrity.
pub fn verify(metadata: &DatasetMetadata, data: Option<&[u8]>) -> Result<VerificationReport> {
    // 1. Verify provider DID signature on metadata
    let signature_valid = verify_signature(metadata);

    // 2. If data provided, verify hash matches info_hash
    let integrity_valid = match data {
        Some(bytes) => {
            let hash = hex::encode(Sha256::digest(bytes));
            hash == metadata.info_hash
        }
        None => false,
    };

    let trust_level = match (signature_valid, integrity_valid) {
        (true, true) => TrustLevel::L2SelfClaim,
        (true, false) => TrustLevel::L1Integrity,
        _ => TrustLevel::L0Untrusted,
    };

    Ok(VerificationReport {
        signature_valid,
        integrity_valid,
        trust_level,
    })
}

fn verify_signature(metadata: &DatasetMetadata) -> bool {
    let pubkey = match NodeIdentity::pubkey_from_did(&metadata.provider) {
        Ok(pk) => pk,
        Err(_) => return false,
    };
    let canonical = metadata.canonical_bytes();
    NodeIdentity::verify(&pubkey, &canonical, &metadata.signature).is_ok()
}
