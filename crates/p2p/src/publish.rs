use anyhow::Result;
use data_auth::privacy::{sanitize_metadata, PrivacyConfig};
use data_core::identity::{sha256_hex, NodeIdentity};
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;
use sha2::Digest;
use std::path::Path;
use tracing::info;

use crate::dht::DhtIndex;
use crate::storage::MetadataStore;

/// Publish a local file as a dataset to the P2P network.
/// This is the core auto-publish pipeline:
///   file → read → hash → metadata → privacy sanitize → sign → store → DHT PUT → GossipSub broadcast
pub async fn publish_file(
    path: &Path,
    identity: &NodeIdentity,
    dht: &DhtIndex,
    store: &MetadataStore,
    access: AccessMode,
    price: f64,
) -> Result<DatasetMetadata> {
    publish_file_with_privacy(path, identity, dht, store, access, price, &PrivacyConfig::default(), false).await
}

/// Publish with explicit privacy configuration.
pub async fn publish_file_with_privacy(
    path: &Path,
    identity: &NodeIdentity,
    dht: &DhtIndex,
    store: &MetadataStore,
    access: AccessMode,
    price: f64,
    privacy: &PrivacyConfig,
    ephemeral_did: bool,
) -> Result<DatasetMetadata> {
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    info!(file = %path.display(), "publishing dataset");

    // 1. Read file bytes
    let data = std::fs::read(path)?;

    // 2. Compute content hash → CID
    let content_hash = sha256_hex(&data);
    let cid = DatasetCid(content_hash.clone());

    // 3. Compute Merkle-style info hash (SHA-256 of entire file for M1; proper piece tree later)
    let info_hash = content_hash.clone();

    // 4. Infer basic schema from file extension
    let (schema, tags) = infer_schema(path, &data)?;

    // 5. Use ephemeral DID if configured (prevents cross-dataset correlation)
    let signing_identity = if ephemeral_did {
        identity.derive_ephemeral(&cid.0)
    } else {
        NodeIdentity::from_seed(identity.seed())
    };

    // 6. Build metadata
    let now = chrono::Utc::now();
    let license = License {
        spdx_id: "CC-BY-4.0".into(),
        commercial_use: true,
        derivative_allowed: true,
    };

    let metadata = DatasetMetadata {
        cid: cid.clone(),
        info_hash,
        title: file_name.clone(),
        description: None,
        tags,
        schema,
        stats: None,
        access,
        price: if price > 0.0 { Price::usdc(price) } else { Price::free() },
        license,
        provider: signing_identity.did.clone(),
        signature: String::new(), // filled below
        provenance: Provenance::Original,
        created_at: now,
        updated_at: now,
        verifiable_credential: None,
    };

    // 7. Apply privacy sanitization (DP noise, column hashing, tag filtering)
    let sanitized = sanitize_metadata(&metadata, privacy)?;

    // 8. Sign the sanitized metadata (this is what gets published)
    let canonical = sanitized.canonical_bytes();
    let mut published = sanitized;
    published.signature = signing_identity.sign(&canonical);

    // 9. Store original locally (for owner's reference), publish sanitized
    store.put(&metadata)?;
    store.put_file_path(&cid, path)?;

    // 10. DHT PUT (sanitized)
    dht.put_metadata(&published).await?;

    // 11. GossipSub broadcast (sanitized)
    dht.broadcast_metadata(&published).await?;

    info!(
        cid = %cid.0,
        title = %file_name,
        rows = published.schema.row_count,
        size = published.schema.size_bytes,
        privacy = ?privacy.level,
        ephemeral_did = ephemeral_did,
        "✅ dataset published"
    );

    Ok(published)
}

/// Infer a basic schema from file content. For M1 we do minimal CSV parsing.
fn infer_schema(path: &Path, data: &[u8]) -> Result<(DatasetSchema, Vec<String>)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let size_bytes = data.len() as u64;

    match ext {
        "csv" | "tsv" => {
            let separator = if ext == "tsv" { b'\t' } else { b',' };
            let text = String::from_utf8_lossy(data);
            let mut lines = text.lines();

            // Parse header
            let header = lines.next().unwrap_or("");
            let sep_char = separator as char;
            let col_names: Vec<&str> = header.split(sep_char).collect();

            let row_count = lines.count() as u64; // remaining lines = data rows

            let columns: Vec<ColumnDef> = col_names
                .iter()
                .map(|name| ColumnDef {
                    name: name.trim().to_string(),
                    dtype: "utf8".into(), // default; could be refined
                    nullable: true,
                    description: None,
                })
                .collect();

            let tags: Vec<String> = col_names
                .iter()
                .map(|n| n.trim().to_lowercase().replace(' ', "_"))
                .collect();

            Ok((
                DatasetSchema { columns, row_count, size_bytes },
                tags,
            ))
        }
        "json" => {
            // Count top-level array elements or lines
            let row_count = data.iter().filter(|&&b| b == b'\n').count().max(1) as u64;
            Ok((
                DatasetSchema { columns: vec![], row_count, size_bytes },
                vec!["json".into()],
            ))
        }
        "parquet" => {
            // For M1, just record size; proper Parquet parsing in M2 with Polars
            Ok((
                DatasetSchema { columns: vec![], row_count: 0, size_bytes },
                vec!["parquet".into()],
            ))
        }
        _ => Ok((
            DatasetSchema { columns: vec![], row_count: 0, size_bytes },
            vec![],
        )),
    }
}
