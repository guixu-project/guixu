// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use data_core::types::{ArtifactType, DatasetArtifact, DatasetCid};
use serde_json::json;

use crate::state::AppState;

fn skill_id_from_cid(cid: &str) -> Option<&str> {
    let rest = cid.strip_prefix("skill:")?;
    let (skill_id, _) = rest.split_once(':')?;
    Some(skill_id)
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
        let items = state.search_engine.lookup_by_skill(skill_id, cid).await?;
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
        let items = state.search_engine.lookup_by_skill(&skill_id, cid).await?;
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
        let items = state
            .search_engine
            .schema_probe_by_skill(skill_id, cid)
            .await?;
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
        let items = state
            .search_engine
            .schema_probe_by_skill(&skill_id, cid)
            .await?;
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

pub async fn query(args: serde_json::Value, state: &AppState) -> Result<String> {
    let cid = args
        .get("cid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing cid"))?;
    let question = args
        .get("question")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing question"))?;

    let skill_id = skill_id_from_cid(cid)
        .ok_or_else(|| anyhow!("dataset_query requires a skill-backed CID"))?;

    let result = state
        .search_engine
        .query_by_skill(skill_id, cid, question)
        .await?;

    Ok(serde_json::to_string_pretty(&json!({
        "cid": cid,
        "skill_id": skill_id,
        "question": question,
        "result": result,
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
    let items = state
        .search_engine
        .download_by_skill(&skill_id, cid)
        .await?;

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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use data_core::types::{SearchResult, SkillCapability, SourceFamily};
    use data_search::adapters::ExternalAdapter;
    use data_search::engine::SearchEngine;
    use data_search::intent::IntentParser;
    use data_search::vector_index::VectorIndex;

    use super::*;

    struct HandlerMultiOpStub;

    #[async_trait::async_trait]
    impl ExternalAdapter for HandlerMultiOpStub {
        fn name(&self) -> &str {
            "handler_multi_op"
        }

        fn skill_id(&self) -> &str {
            "handler_multi_op"
        }

        fn source_family(&self) -> SourceFamily {
            SourceFamily::Custom
        }

        fn capabilities(&self) -> Vec<SkillCapability> {
            vec![
                SkillCapability::Search,
                SkillCapability::Lookup,
                SkillCapability::Download,
                SkillCapability::SchemaProbe,
            ]
        }

        async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SearchResult>> {
            Ok(vec![])
        }

        async fn lookup(&self, id: &str) -> Result<Vec<serde_json::Value>> {
            Ok(vec![serde_json::json!({"name": "record.json", "id": id})])
        }

        async fn download(&self, id: &str) -> Result<Vec<serde_json::Value>> {
            Ok(vec![serde_json::json!({
                "name": "archive.parquet",
                "download_url": format!("https://example.test/{id}"),
                "content_type": "application/octet-stream"
            })])
        }

        async fn schema_probe(&self, id: &str) -> Result<Vec<serde_json::Value>> {
            Ok(vec![serde_json::json!({
                "name": "schema.json",
                "url": format!("https://example.test/{id}/schema"),
                "content_type": "application/json"
            })])
        }
    }

    #[tokio::test]
    async fn handler_routes_multi_operation_calls_through_search_engine() {
        let mut state = AppState::for_codex().await.unwrap();
        state.search_engine = Arc::new(SearchEngine::new(
            VectorIndex,
            IntentParser,
            vec![Box::new(HandlerMultiOpStub)],
        ));

        let cid = "skill:handler_multi_op:dataset-42";

        let lookup_output = lookup(serde_json::json!({ "cid": cid }), &state)
            .await
            .unwrap();
        let schema_output = schema_probe(serde_json::json!({ "cid": cid }), &state)
            .await
            .unwrap();
        let download_output = download_via_skill(serde_json::json!({ "cid": cid }), &state)
            .await
            .unwrap();

        let lookup_json: serde_json::Value = serde_json::from_str(&lookup_output).unwrap();
        let schema_json: serde_json::Value = serde_json::from_str(&schema_output).unwrap();
        let download_json: serde_json::Value = serde_json::from_str(&download_output).unwrap();

        assert_eq!(lookup_json["skill_id"], "handler_multi_op");
        assert_eq!(lookup_json["items"][0]["id"], cid);
        assert_eq!(schema_json["artifacts"][0]["artifact_type"], "schema");
        assert_eq!(download_json["artifacts"][0]["artifact_type"], "download");
        assert_eq!(download_json["status"], "completed");
    }
}
