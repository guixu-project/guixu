// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use data_search::intent::{IntentParser, QueryProfiler};

use crate::sampling_impls::SamplingIntentParser;
use crate::server::AppState;

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
    let raw_query = args
        .get("raw_query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let query = raw_query
        .or(query)
        .ok_or_else(|| anyhow::anyhow!("missing query"))?;

    // Prefer host sampling; fall back to direct DeepSeek API if configured.
    let sampling = state.sampling_handle.read().unwrap().clone();
    let profile = if let Some(handle) = sampling.as_ref() {
        let parser = SamplingIntentParser::new(handle.clone());
        parser.profile(query).await?
    } else {
        let parser = IntentParser::default();
        parser.profile(query).await?
    };

    Ok(serde_json::to_string_pretty(&profile)?)
}
