// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde_json::json;

use crate::server::AppState;

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
    let info_hash = args
        .get("info_hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing info_hash"))?;

    // GIP005: P2PHandle initializes lazily on first use
    let downloaded_to = state
        .p2p_handle
        .download_and_seed(info_hash, &state.store)
        .await?;

    Ok(serde_json::to_string_pretty(&json!({
        "info_hash": info_hash,
        "status": "completed",
        "downloaded_to": downloaded_to.to_string_lossy(),
        "auto_seeding": true
    }))?)
}

pub async fn handle_preview(args: serde_json::Value, state: &AppState) -> Result<String> {
    let info_hash = args
        .get("info_hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing info_hash"))?;
    let max_bytes = args
        .get("max_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(65536) as usize;

    // GIP005: P2PHandle initializes lazily on first use
    let bytes = state
        .p2p_handle
        .download_preview(info_hash, max_bytes)
        .await?;

    if bytes.is_empty() {
        anyhow::bail!("preview not available — torrent may have no seeders");
    }

    // Try to interpret as UTF-8 text; fall back to hex dump
    let preview = match String::from_utf8(bytes.clone()) {
        Ok(text) => {
            // Truncate to max_bytes worth of chars
            let truncated: String = text.chars().take(max_bytes).collect();
            truncated
        }
        Err(_) => {
            // Binary file — show hex dump of the first 512 bytes
            let slice = &bytes[..bytes.len().min(512)];
            slice
                .chunks(16)
                .map(|chunk| {
                    let hex_part: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
                    let ascii_part: String = chunk
                        .iter()
                        .map(|&b| {
                            if b.is_ascii_graphic() || b == b' ' {
                                b as char
                            } else {
                                '.'
                            }
                        })
                        .collect();
                    format!("{:<48} {}", hex_part.join(" "), ascii_part)
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    };

    Ok(serde_json::to_string_pretty(&json!({
        "info_hash": info_hash,
        "preview": preview,
        "bytes_read": bytes.len(),
        "is_text": String::from_utf8(bytes).is_ok(),
    }))?)
}

pub async fn handle_stats(args: serde_json::Value, state: &AppState) -> Result<String> {
    let info_hash = args
        .get("info_hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing info_hash"))?;

    let stats = state.p2p_handle.get_stats(info_hash)?;

    Ok(serde_json::to_string_pretty(&stats)?)
}
