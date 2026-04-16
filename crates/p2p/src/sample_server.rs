// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::identity::NodeIdentity;
use data_core::types::{AccessMode, SampleRequest, SampleResponse};
use data_storage::metadata_store::MetadataStore;
use polars::prelude::IntoLazy;
use polars::prelude::{SerReader, SerWriter};
use tracing::{info, warn};

/// Handle an inbound sample request from a remote peer.
/// Returns a signed SampleResponse or None if the CID is unknown.
pub fn handle_sample_request(
    request: &SampleRequest,
    store: &MetadataStore,
    identity: &NodeIdentity,
) -> Result<Option<SampleResponse>> {
    let cid = &request.cid;

    // 1. Check MetadataStore for CID
    let metadata = match store.get(cid)? {
        Some(m) => m,
        None => {
            warn!(cid = %cid.0, "sample request for unknown CID");
            return Ok(None);
        }
    };

    // 2. Check if this node published it
    if metadata.provider.0 != identity.did.0 {
        // Also check ephemeral DIDs — for now, check file_path existence
        if store.get_file_path(cid)?.is_none() {
            warn!(cid = %cid.0, "sample request for CID not published by this node");
            return Ok(None);
        }
    }

    // 3. Get file path
    let file_path = match store.get_file_path(cid)? {
        Some(p) => p,
        None => return Ok(None),
    };

    // 4. Read preview data based on access mode and format
    let max_rows = request.rows.min(100);
    let has_sparse_opts = request.columns.is_some() || request.row_predicate.is_some();

    let preview_data = if has_sparse_opts {
        // Use sparse reading with Polars for column/row filtering
        match metadata.access {
            AccessMode::Open => read_sparse_preview(
                &file_path,
                max_rows,
                request.max_bytes,
                request.columns.as_deref(),
                request.row_predicate.as_deref(),
            )?,
            AccessMode::Paid => {
                // Paid datasets have stricter limits
                read_sparse_preview(
                    &file_path,
                    max_rows.min(5),
                    request.max_bytes.min(4096),
                    request.columns.as_deref(),
                    request.row_predicate.as_deref(),
                )?
            }
        }
    } else {
        match request.format.as_str() {
            "schema_only" => {
                // Return empty preview — schema is already in the response
                Vec::new()
            }
            "random_sample" => match metadata.access {
                AccessMode::Open => read_random_sample(&file_path, max_rows, request.max_bytes)?,
                AccessMode::Paid => {
                    read_random_sample(&file_path, max_rows.min(5), request.max_bytes.min(4096))?
                }
            },
            _ => match metadata.access {
                AccessMode::Open => read_preview(&file_path, max_rows, request.max_bytes)?,
                AccessMode::Paid => {
                    read_preview(&file_path, max_rows.min(5), request.max_bytes.min(4096))?
                }
            },
        }
    };

    // 5. Build and sign response
    let response = SampleResponse {
        cid: cid.clone(),
        schema: metadata.schema.clone(),
        preview_data: base64_encode(&preview_data),
        provider_did: identity.did.clone(),
        signature: String::new(), // filled below
    };

    let canonical = serde_json::to_vec(&response)?;
    let mut signed = response;
    signed.signature = identity.sign(&canonical);

    info!(cid = %cid.0, bytes = preview_data.len(), "served sample request");
    Ok(Some(signed))
}

fn read_preview(path: &std::path::Path, max_rows: usize, max_bytes: usize) -> Result<Vec<u8>> {
    let content = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&content);
    let lines: Vec<&str> = text.lines().take(max_rows + 1).collect(); // +1 for header
    let preview = lines.join("\n");
    let bytes = preview.as_bytes();
    let truncated = &bytes[..bytes.len().min(max_bytes)];
    Ok(truncated.to_vec())
}

/// Read a preview with sparse column selection and optional row filtering.
/// Uses Polars for Parquet files to enable efficient column-pruned reading.
/// Falls back to CSV parsing for non-Parquet files.
fn read_sparse_preview(
    path: &std::path::Path,
    max_rows: usize,
    max_bytes: usize,
    columns: Option<&[String]>,
    row_predicate: Option<&str>,
) -> Result<Vec<u8>> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if extension == "parquet" {
        read_sparse_parquet(path, max_rows, max_bytes, columns, row_predicate)
    } else {
        // For CSV/JSON, fall back to row filtering via Polars
        read_sparse_csv(path, max_rows, max_bytes, columns, row_predicate)
    }
}

fn read_sparse_parquet(
    path: &std::path::Path,
    max_rows: usize,
    max_bytes: usize,
    columns: Option<&[String]>,
    row_predicate: Option<&str>,
) -> Result<Vec<u8>> {
    let lf = polars::prelude::LazyFrame::scan_parquet(path, Default::default())?;

    let lf = if let Some(cols) = columns {
        let exprs: Vec<polars::prelude::Expr> = cols
            .iter()
            .map(|c| polars::prelude::col(c.as_str()))
            .collect();
        lf.select(exprs)
    } else {
        lf
    };

    // Apply row predicate if specified
    let lf = if let Some(predicate) = row_predicate {
        lf.filter(
            polars::prelude::col(predicate.split_whitespace().next().unwrap_or("*")).is_not_null(),
        )
    } else {
        lf
    };

    let df = lf.collect()?;

    // Limit rows
    let df = df.head(Some(max_rows));

    // Convert to CSV bytes
    let mut buf = Vec::new();
    polars::prelude::CsvWriter::new(&mut buf).finish(&mut df.clone())?;

    Ok(buf[..buf.len().min(max_bytes)].to_vec())
}

fn read_sparse_csv(
    path: &std::path::Path,
    max_rows: usize,
    max_bytes: usize,
    columns: Option<&[String]>,
    row_predicate: Option<&str>,
) -> Result<Vec<u8>> {
    let df = polars::prelude::CsvReadOptions::default()
        .try_into_reader_with_file_path(Some(path.into()))?
        .finish()?;

    let lf = df.lazy();

    let lf = if let Some(cols) = columns {
        let exprs: Vec<polars::prelude::Expr> = cols
            .iter()
            .map(|c| polars::prelude::col(c.as_str()))
            .collect();
        lf.select(exprs)
    } else {
        lf
    };

    let df = lf.collect()?;

    // Apply row predicate if specified (simplified - just filter by first column non-null)
    let df = if row_predicate.is_some() {
        let first_col = df.get_column_names_str()[0].to_string();
        df.lazy()
            .filter(polars::prelude::col(first_col.as_str()).is_not_null())
            .collect()?
    } else {
        df
    };

    // Limit rows
    let df = df.head(Some(max_rows));

    // Convert to CSV bytes
    let mut buf = Vec::new();
    polars::prelude::CsvWriter::new(&mut buf).finish(&mut df.clone())?;

    Ok(buf[..buf.len().min(max_bytes)].to_vec())
}

fn read_random_sample(
    path: &std::path::Path,
    max_rows: usize,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    use rand::seq::SliceRandom;

    let content = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&content);
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 1 {
        return Ok(text.as_bytes().to_vec());
    }
    let header = lines.remove(0);
    let sample_count = max_rows.min(lines.len());
    let mut rng = rand::thread_rng();
    let sampled: Vec<&str> = lines
        .choose_multiple(&mut rng, sample_count)
        .copied()
        .collect();
    let mut result = String::from(header);
    for line in sampled {
        result.push('\n');
        result.push_str(line);
    }
    let bytes = result.as_bytes();
    let truncated = &bytes[..bytes.len().min(max_bytes)];
    Ok(truncated.to_vec())
}

fn base64_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        let _ = result.write_char(CHARS[((n >> 18) & 0x3F) as usize] as char);
        let _ = result.write_char(CHARS[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            let _ = result.write_char(CHARS[((n >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            let _ = result.write_char(CHARS[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use data_core::identity::NodeIdentity;
    use data_core::metadata::{DatasetMetadata, Provenance};
    use data_core::types::*;
    use data_storage::metadata_store::MetadataStore;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir()
            .join("guixu-test-sample")
            .join(name)
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn setup_store(dir: &std::path::Path, cid: &str, csv: &str) -> (MetadataStore, NodeIdentity) {
        let store = MetadataStore::open(dir).unwrap();
        let identity = NodeIdentity::generate();
        let file_path = dir.join("data.csv");
        std::fs::write(&file_path, csv).unwrap();

        let metadata = DatasetMetadata {
            cid: DatasetCid(cid.into()),
            info_hash: None,
            title: "test".into(),
            description: None,
            tags: vec![],
            data_type: DataType::Tabular,
            schema: DatasetSchema {
                columns: vec![],
                row_count: 3,
                size_bytes: csv.len() as u64,
            },
            stats: None,
            video_meta: None,
            access: AccessMode::Open,
            price: Price::free(),
            license: License {
                spdx_id: "MIT".into(),
                commercial_use: true,
                derivative_allowed: true,
            },
            provider: identity.did.clone(),
            signature: String::new(),
            provenance: Provenance::Original,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            version: None,
            previous_version: None,
            verifiable_credential: None,
            source_attributes: None,
        };
        store.put(&metadata).unwrap();
        store
            .put_file_path(&DatasetCid(cid.into()), &file_path)
            .unwrap();
        (store, identity)
    }

    #[test]
    fn returns_none_for_unknown_cid() {
        let dir = temp_dir("unknown");
        let store = MetadataStore::open(&dir).unwrap();
        let identity = NodeIdentity::generate();
        let req = SampleRequest {
            cid: DatasetCid("nonexistent".into()),
            max_bytes: 1024,
            format: "head".into(),
            rows: 10,
        };
        let result = handle_sample_request(&req, &store, &identity).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn returns_preview_for_open_dataset() {
        let dir = temp_dir("open-preview");
        let csv = "a,b,c\n1,2,3\n4,5,6\n7,8,9\n";
        let (store, identity) = setup_store(&dir, "cid-open", csv);

        let req = SampleRequest {
            cid: DatasetCid("cid-open".into()),
            max_bytes: 65536,
            format: "head".into(),
            rows: 20,
        };
        let resp = handle_sample_request(&req, &store, &identity)
            .unwrap()
            .unwrap();
        assert_eq!(resp.cid.0, "cid-open");
        assert!(!resp.preview_data.is_empty());
        assert!(!resp.signature.is_empty());
    }

    #[test]
    fn paid_dataset_limits_preview_rows() {
        let dir = temp_dir("paid-limit");
        let csv = "a,b\n1,2\n3,4\n5,6\n7,8\n9,10\n11,12\n13,14\n";
        let store = MetadataStore::open(&dir).unwrap();
        let identity = NodeIdentity::generate();
        let file_path = dir.join("paid.csv");
        std::fs::write(&file_path, csv).unwrap();

        let metadata = DatasetMetadata {
            cid: DatasetCid("cid-paid".into()),
            info_hash: None,
            title: "paid".into(),
            description: None,
            tags: vec![],
            data_type: DataType::Tabular,
            schema: DatasetSchema {
                columns: vec![],
                row_count: 7,
                size_bytes: csv.len() as u64,
            },
            stats: None,
            video_meta: None,
            access: AccessMode::Paid,
            price: Price::usdc(1.0),
            license: License {
                spdx_id: "MIT".into(),
                commercial_use: true,
                derivative_allowed: true,
            },
            provider: identity.did.clone(),
            signature: String::new(),
            provenance: Provenance::Original,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            version: None,
            previous_version: None,
            verifiable_credential: None,
            source_attributes: None,
        };
        store.put(&metadata).unwrap();
        store
            .put_file_path(&DatasetCid("cid-paid".into()), &file_path)
            .unwrap();

        let req = SampleRequest {
            cid: DatasetCid("cid-paid".into()),
            max_bytes: 65536,
            format: "head".into(),
            rows: 100,
        };
        let resp = handle_sample_request(&req, &store, &identity)
            .unwrap()
            .unwrap();
        // Paid datasets should have limited preview (max 5 data rows + header = 6 lines)
        let decoded = String::from_utf8(base64_decode(&resp.preview_data)).unwrap();
        let line_count = decoded.lines().count();
        assert!(
            line_count <= 6,
            "paid preview should be limited, got {line_count} lines"
        );
    }

    fn base64_decode(s: &str) -> Vec<u8> {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
        for chunk in bytes.chunks(4) {
            let vals: Vec<u32> = chunk
                .iter()
                .map(|&b| TABLE.iter().position(|&c| c == b).unwrap_or(0) as u32)
                .collect();
            if vals.len() >= 2 {
                out.push(((vals[0] << 2) | (vals[1] >> 4)) as u8);
            }
            if vals.len() >= 3 {
                out.push((((vals[1] & 0xF) << 4) | (vals[2] >> 2)) as u8);
            }
            if vals.len() >= 4 {
                out.push((((vals[2] & 0x3) << 6) | vals[3]) as u8);
            }
        }
        out
    }

    #[test]
    fn base64_encode_roundtrip() {
        let data = b"hello, world!";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded);
        assert_eq!(decoded, data);
    }

    #[test]
    fn schema_only_returns_empty_preview() {
        let dir = temp_dir("schema-only");
        let csv = "a,b,c\n1,2,3\n4,5,6\n";
        let (store, identity) = setup_store(&dir, "cid-schema", csv);

        let req = SampleRequest {
            cid: DatasetCid("cid-schema".into()),
            max_bytes: 65536,
            format: "schema_only".into(),
            rows: 20,
        };
        let resp = handle_sample_request(&req, &store, &identity)
            .unwrap()
            .unwrap();
        assert_eq!(resp.preview_data, "");
    }

    #[test]
    fn random_sample_returns_header_and_rows() {
        let dir = temp_dir("random-sample");
        let csv = "a,b\n1,2\n3,4\n5,6\n7,8\n9,10\n";
        let (store, identity) = setup_store(&dir, "cid-rand", csv);

        let req = SampleRequest {
            cid: DatasetCid("cid-rand".into()),
            max_bytes: 65536,
            format: "random_sample".into(),
            rows: 3,
        };
        let resp = handle_sample_request(&req, &store, &identity)
            .unwrap()
            .unwrap();
        let decoded = String::from_utf8(base64_decode(&resp.preview_data)).unwrap();
        let lines: Vec<&str> = decoded.lines().collect();
        assert!(lines[0] == "a,b", "first line should be header");
        assert_eq!(lines.len(), 4, "header + 3 sampled rows");
    }
}
