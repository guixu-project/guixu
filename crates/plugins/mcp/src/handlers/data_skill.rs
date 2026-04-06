// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use serde_json::json;

use crate::state::AppState;

pub async fn lookup(args: serde_json::Value, state: &AppState) -> Result<String> {
    let cid = args
        .get("cid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing cid"))?;

    let metadata = state
        .store
        .list_all()?
        .into_iter()
        .find(|item| item.cid.0 == cid)
        .ok_or_else(|| anyhow!("dataset not found: {cid}"))?;

    Ok(serde_json::to_string_pretty(&json!({
        "cid": metadata.cid.0,
        "title": metadata.title,
        "description": metadata.description,
        "tags": metadata.tags,
        "data_type": metadata.data_type,
        "schema": metadata.schema,
        "price": metadata.price,
        "license": metadata.license,
        "provider": metadata.provider,
        "source_attributes": metadata.source_attributes,
    }))?)
}

pub async fn schema_probe(args: serde_json::Value, state: &AppState) -> Result<String> {
    let cid = args
        .get("cid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing cid"))?;

    let metadata = state
        .store
        .list_all()?
        .into_iter()
        .find(|item| item.cid.0 == cid)
        .ok_or_else(|| anyhow!("dataset not found: {cid}"))?;

    Ok(serde_json::to_string_pretty(&json!({
        "cid": metadata.cid.0,
        "schema": metadata.schema,
        "data_type": metadata.data_type,
    }))?)
}

pub async fn download_via_skill(args: serde_json::Value, _state: &AppState) -> Result<String> {
    let cid = args
        .get("cid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing cid"))?;

    Ok(serde_json::to_string_pretty(&json!({
        "cid": cid,
        "status": "accepted",
        "message": "Skill-backed download execution path is scaffolded. Full operation dispatch is the next runtime step."
    }))?)
}
