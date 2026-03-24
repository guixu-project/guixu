use anyhow::Result;
use data_core::identity::{sha256_hex, NodeIdentity};
use data_core::metadata::{DatasetMetadata, Provenance};
use data_core::types::*;
use sha2::{Digest, Sha256};
use std::path::Path;
use tracing::info;

use crate::dht::DhtIndex;
use crate::storage::MetadataStore;

/// Publish a local file as a dataset to the P2P network.
/// This is the core auto-publish pipeline:
///   file → read → hash → metadata → sign → store → DHT PUT → GossipSub broadcast
pub async fn publish_file(
    path: &Path,
    identity: &NodeIdentity,
    dht: &DhtIndex,
    store: &MetadataStore,
    access: AccessMode,
    price: f64,
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

    // 5. Build metadata
    let now = chrono::Utc::now();
    let license = License {
        spdx_id: "CC-BY-4.0".into(),
        commercial_use: true,
        derivative_allowed: true,
    };

    let mut metadata = DatasetMetadata {
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
        provider: identity.did.clone(),
        signature: String::new(), // filled below
        provenance: Provenance::Original,
        created_at: now,
        updated_at: now,
        verifiable_credential: None,
    };

    // 6. Sign metadata
    let canonical = metadata.canonical_bytes();
    metadata.signature = identity.sign(&canonical);

    // 7. Store locally
    store.put(&metadata)?;
    store.put_file_path(&cid, path)?;

    // 8. DHT PUT
    dht.put_metadata(&metadata).await?;

    // 9. GossipSub broadcast
    dht.broadcast_metadata(&metadata).await?;

    info!(
        cid = %cid.0,
        title = %file_name,
        rows = metadata.schema.row_count,
        size = metadata.schema.size_bytes,
        "✅ dataset published"
    );

    Ok(metadata)
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
