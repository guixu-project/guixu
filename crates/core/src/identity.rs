use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::error::{DataProtocolError, Result};
use crate::types::Did;

/// Node identity backed by an Ed25519 keypair.
pub struct NodeIdentity {
    signing_key: SigningKey,
    pub did: Did,
}

impl NodeIdentity {
    /// Generate a new random identity.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let did = Self::did_from_verifying_key(&signing_key.verifying_key());
        Self { signing_key, did }
    }

    /// Restore from a 32-byte secret seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(seed);
        let did = Self::did_from_verifying_key(&signing_key.verifying_key());
        Self { signing_key, did }
    }

    /// Secret seed bytes (for persistence).
    pub fn seed(&self) -> &[u8; 32] {
        self.signing_key.as_bytes()
    }

    /// Sign arbitrary bytes, return hex-encoded signature.
    pub fn sign(&self, msg: &[u8]) -> String {
        let sig = self.signing_key.sign(msg);
        hex::encode(sig.to_bytes())
    }

    /// Public verifying key bytes.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Verify a hex-encoded signature against a public key.
    pub fn verify(public_key: &[u8; 32], msg: &[u8], sig_hex: &str) -> Result<()> {
        let sig_bytes = hex::decode(sig_hex)
            .map_err(|e| DataProtocolError::AuthFailed(format!("bad sig hex: {e}")))?;
        let sig = ed25519_dalek::Signature::from_slice(&sig_bytes)
            .map_err(|e| DataProtocolError::AuthFailed(format!("bad sig: {e}")))?;
        let vk = VerifyingKey::from_bytes(public_key)
            .map_err(|e| DataProtocolError::AuthFailed(format!("bad pubkey: {e}")))?;
        vk.verify(msg, &sig)
            .map_err(|_| DataProtocolError::VerificationFailed("signature mismatch".into()))
    }

    /// Extract public key bytes from a did:key string.
    pub fn pubkey_from_did(did: &Did) -> Result<[u8; 32]> {
        let z_part = did
            .0
            .strip_prefix("did:key:z")
            .ok_or_else(|| DataProtocolError::Identity("invalid did:key format".into()))?;
        let decoded = bs58::decode(z_part)
            .into_vec()
            .map_err(|e| DataProtocolError::Identity(format!("bs58 decode: {e}")))?;
        // skip 2-byte multicodec prefix (0xed, 0x01)
        if decoded.len() < 34 || decoded[0] != 0xed || decoded[1] != 0x01 {
            return Err(DataProtocolError::Identity("bad multicodec prefix".into()));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&decoded[2..34]);
        Ok(key)
    }

    fn did_from_verifying_key(vk: &VerifyingKey) -> Did {
        let mut buf = vec![0xed, 0x01];
        buf.extend_from_slice(&vk.to_bytes());
        Did(format!("did:key:z{}", bs58::encode(&buf).into_string()))
    }

    /// Derive an ephemeral identity for a specific dataset CID.
    ///
    /// Uses HMAC-like derivation: `child_seed = SHA-256(parent_seed || cid)`.
    /// This produces a unique, unlinkable DID per dataset while allowing the
    /// owner to prove ownership by re-deriving from the master seed.
    pub fn derive_ephemeral(&self, dataset_cid: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(self.signing_key.as_bytes());
        hasher.update(dataset_cid.as_bytes());
        let child_seed: [u8; 32] = hasher.finalize().into();
        Self::from_seed(&child_seed)
    }
}

/// Compute SHA-256 hash of bytes, return hex string.
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}
