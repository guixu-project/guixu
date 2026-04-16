// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::identity::NodeIdentity;
use data_core::types::{AccessMode, SampleRequest, SampleResponse};
use data_storage::metadata_store::MetadataStore;
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

    // 4. Read preview data based on access mode
    let max_rows = request.rows.min(100);
    let preview_data = match metadata.access {
        AccessMode::Open => read_preview(&file_path, max_rows, request.max_bytes)?,
        AccessMode::Paid => {
            // For paid datasets, return limited preview (schema + first 5 rows)
            read_preview(&file_path, max_rows.min(5), request.max_bytes.min(4096))?
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
