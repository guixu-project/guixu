// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use data_search::intent::QueryProfile;

use crate::server::AppState;

pub async fn handle(args: serde_json::Value, _state: &AppState) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required field: query"))?;

    let task_type = args
        .get("task_type")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required field: task_type — must be one of: classification, detection, segmentation, forecasting, ranking, retrieval, generation, summarization, evaluation"))?;

    let keywords: Vec<String> = args
        .get("keywords")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    if keywords.is_empty() {
        anyhow::bail!("keywords must not be empty — extract dataset content terms from the query (e.g. 'cat', 'lung nodule'), NOT task words");
    }

    let sample_unit = args
        .get("sample_unit")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let task_description = args
        .get("task_description")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let target_entity = args
        .get("target_entity")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let budget = args
        .get("budget")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "0 USD".to_string());

    let profile = QueryProfile {
        raw_query: query.to_string(),
        task_type: Some(task_type.to_string()),
        task_description: task_description.or_else(|| {
            Some(format!(
                "{task_type} task for {}",
                target_entity.as_deref().unwrap_or(query)
            ))
        }),
        target_entity,
        keywords,
        data_standard: data_search::intent::DataStandard {
            sample_unit,
            budget,
            ..Default::default()
        },
        user_profile: Default::default(),
    };

    Ok(serde_json::to_string_pretty(&profile)?)
}
