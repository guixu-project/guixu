// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use data_core::types::{ArtifactType, DatasetArtifact, DatasetCid};
use data_search::adapters::{execute_skill_operation, load_open_data_skills};
use serde_json::json;
use std::sync::Arc;

use crate::state::AppState;

fn skill_id_from_cid(cid: &str) -> Option<&str> {
    let rest = cid.strip_prefix("skill:")?;
    let (skill_id, _) = rest.split_once(':')?;
    Some(skill_id)
}

fn find_skill(skill_id: &str) -> Result<data_search::adapters::OpenDataSkillSpec> {
    load_open_data_skills()?
        .into_iter()
        .find(|skill| skill.id == skill_id)
        .ok_or_else(|| anyhow!("open data skill not found: {skill_id}"))
}

fn skill_id_from_metadata(metadata: &data_core::metadata::DatasetMetadata) -> Option<String> {
    metadata
        .source_attributes
        .as_ref()
        .and_then(|value| value.get("skill_id"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn artifact_from_item(
    cid: &str,
    item: serde_json::Value,
    index: usize,
    artifact_type: ArtifactType,
) -> DatasetArtifact {
    let name = item
        .get("name")
        .or_else(|| item.get("filename"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string);
    let url = item
        .get("url")
        .or_else(|| item.get("download_url"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string);
    let content_type = item
        .get("content_type")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);
    let size_bytes = item.get("size_bytes").and_then(|v| v.as_u64());
    let checksum = item
        .get("checksum")
        .or_else(|| item.get("sha256"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string);

    DatasetArtifact {
        artifact_id: format!("artifact-{index}"),
        dataset_cid: DatasetCid(cid.to_string()),
        artifact_type,
        name,
        url,
        content_type,
        size_bytes,
        checksum,
        metadata: Some(item),
    }
}

pub async fn lookup(args: serde_json::Value, state: &AppState) -> Result<String> {
    let cid = args
        .get("cid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing cid"))?;

    if let Some(skill_id) = skill_id_from_cid(cid) {
        let skill = find_skill(skill_id)?;
        let items = execute_skill_operation(&skill, "lookup", cid, 1).await?;
        return Ok(serde_json::to_string_pretty(&json!({
            "cid": cid,
            "skill_id": skill_id,
            "items": items,
        }))?);
    }

    let metadata = state
        .store
        .list_all()?
        .into_iter()
        .find(|item| item.cid.0 == cid)
        .ok_or_else(|| anyhow!("dataset not found: {cid}"))?;

    if let Some(skill_id) = skill_id_from_metadata(&metadata) {
        let skill = find_skill(&skill_id)?;
        let items = execute_skill_operation(&skill, "lookup", cid, 1).await?;
        return Ok(serde_json::to_string_pretty(&json!({
            "cid": cid,
            "skill_id": skill_id,
            "items": items,
        }))?);
    }

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

    if let Some(skill_id) = skill_id_from_cid(cid) {
        let skill = find_skill(skill_id)?;
        let items = execute_skill_operation(&skill, "schema_probe", cid, 1).await?;
        return Ok(serde_json::to_string_pretty(&json!({
            "cid": cid,
            "skill_id": skill_id,
            "artifacts": items
                .into_iter()
                .enumerate()
                .map(|(i, item)| artifact_from_item(cid, item, i, ArtifactType::Schema))
                .collect::<Vec<_>>(),
        }))?);
    }

    let metadata = state
        .store
        .list_all()?
        .into_iter()
        .find(|item| item.cid.0 == cid)
        .ok_or_else(|| anyhow!("dataset not found: {cid}"))?;

    if let Some(skill_id) = skill_id_from_metadata(&metadata) {
        let skill = find_skill(&skill_id)?;
        let items = execute_skill_operation(&skill, "schema_probe", cid, 1).await?;
        return Ok(serde_json::to_string_pretty(&json!({
            "cid": cid,
            "skill_id": skill_id,
            "artifacts": items
                .into_iter()
                .enumerate()
                .map(|(i, item)| artifact_from_item(cid, item, i, ArtifactType::Schema))
                .collect::<Vec<_>>(),
        }))?);
    }

    Ok(serde_json::to_string_pretty(&json!({
        "cid": metadata.cid.0,
        "schema": metadata.schema,
        "data_type": metadata.data_type,
    }))?)
}

pub async fn download_via_skill(args: serde_json::Value, state: &AppState) -> Result<String> {
    let cid = args
        .get("cid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing cid"))?;

    let skill_id = if let Some(skill_id) = skill_id_from_cid(cid) {
        skill_id.to_string()
    } else {
        let metadata = state
            .store
            .list_all()?
            .into_iter()
            .find(|item| item.cid.0 == cid)
            .ok_or_else(|| anyhow!("dataset not found: {cid}"))?;
        skill_id_from_metadata(&metadata).ok_or_else(|| {
            anyhow!(
                "dataset_download_by_skill requires a skill-backed CID or metadata with source_attributes.skill_id"
            )
        })?
    };
    let skill = find_skill(&skill_id)?;
    let items = execute_skill_operation(&skill, "download", cid, 1).await?;

    Ok(serde_json::to_string_pretty(&json!({
        "cid": cid,
        "skill_id": skill_id,
        "status": "completed",
        "artifacts": items
            .into_iter()
            .enumerate()
            .map(|(i, item)| artifact_from_item(cid, item, i, ArtifactType::Download))
            .collect::<Vec<_>>(),
    }))?)
}
