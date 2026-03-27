use anyhow::Result;
use data_core::identity::NodeIdentity;
use data_core::metadata::DatasetMetadata;
use serde::{Deserialize, Serialize};

/// W3C Verifiable Credential for a dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetCredential {
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    #[serde(rename = "type")]
    pub types: Vec<String>,
    pub issuer: String,
    pub credential_subject: CredentialSubject,
    pub proof: CredentialProof,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialSubject {
    pub id: String, // CID
    pub merkle_root: String,
    pub schema_hash: String,
    pub row_count: u64,
    pub null_rate: f64,
    pub provenance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialProof {
    #[serde(rename = "type")]
    pub proof_type: String,
    pub created: String,
    pub verification_method: String,
    pub proof_value: String,
}

impl DatasetCredential {
    /// Issue a VC for a dataset, signed by the node identity.
    pub fn issue(identity: &NodeIdentity, metadata: &DatasetMetadata) -> Result<Self> {
        let subject = CredentialSubject {
            id: format!("cid:{}", metadata.cid.0),
            merkle_root: metadata.info_hash.clone(),
            schema_hash: data_core::identity::sha256_hex(&serde_json::to_vec(&metadata.schema)?),
            row_count: metadata.schema.row_count,
            null_rate: metadata.stats.as_ref().map(|s| s.null_rate).unwrap_or(0.0),
            provenance: serde_json::to_string(&metadata.provenance)?,
        };

        let subject_bytes = serde_json::to_vec(&subject)?;
        let sig = identity.sign(&subject_bytes);

        Ok(Self {
            context: vec!["https://www.w3.org/2018/credentials/v1".into()],
            types: vec!["VerifiableCredential".into(), "DatasetCredential".into()],
            issuer: identity.did.0.clone(),
            credential_subject: subject,
            proof: CredentialProof {
                proof_type: "Ed25519Signature2020".into(),
                created: chrono::Utc::now().to_rfc3339(),
                verification_method: format!("{}#key-1", identity.did.0),
                proof_value: sig,
            },
        })
    }
}
