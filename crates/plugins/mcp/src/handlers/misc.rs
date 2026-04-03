// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use data_core::types::{AccessMode, DatasetCid};

use crate::server::AppState;

pub async fn handle_verify(args: serde_json::Value, state: &AppState) -> Result<String> {
    let cid_str = args.get("cid").and_then(|v| v.as_str()).unwrap_or("");
    let cid = DatasetCid(cid_str.to_string());
    match state.store.get(&cid)? {
        Some(metadata) => {
            let report = data_auth::verifier::verify(&metadata, None)?;
            Ok(format!("{report:?}"))
        }
        None => Ok(format!("Dataset {cid_str} not found in local store")),
    }
}

pub async fn handle_publish(args: serde_json::Value, state: &AppState) -> Result<String> {
    let file_path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
    let path = std::path::Path::new(file_path);
    if !path.exists() {
        anyhow::bail!("File not found: {file_path}");
    }
    let metadata = data_p2p::publish::publish_file(
        path,
        &state.identity,
        &state.dht,
        &state.store,
        AccessMode::Open,
        0.0,
    )
    .await?;
    Ok(serde_json::to_string_pretty(&metadata)?)
}
