// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use data_search::intent::{
    build_intent_user_prompt, parse_intent_response, DataStandard, QueryProfile, QueryProfiler,
};

use crate::discovery::llm::LlmProvider;

pub struct ApiBackedIntentProfiler {
    llm_provider: std::sync::Arc<dyn LlmProvider>,
}

impl ApiBackedIntentProfiler {
    pub fn new(llm_provider: std::sync::Arc<dyn LlmProvider>) -> Self {
        Self { llm_provider }
    }
}

#[async_trait]
impl QueryProfiler for ApiBackedIntentProfiler {
    async fn profile(&self, query: &str) -> Result<QueryProfile> {
        let system_prompt =
            "You are an intent parser for dataset discovery. Return valid JSON only.";
        let user_prompt = build_intent_user_prompt(query)?;
        let response = self
            .llm_provider
            .complete_text(system_prompt, &user_prompt)
            .await?;
        parse_intent_response(query, &response)
    }
}

pub struct HeuristicIntentProfiler;

#[async_trait]
impl QueryProfiler for HeuristicIntentProfiler {
    async fn profile(&self, query: &str) -> Result<QueryProfile> {
        let keywords: Vec<String> = query
            .split_whitespace()
            .map(|token| {
                token
                    .trim_matches(|character: char| !character.is_alphanumeric())
                    .to_lowercase()
            })
            .filter(|token| token.len() > 2)
            .take(5)
            .collect();
        Ok(QueryProfile {
            raw_query: query.to_string(),
            task_type: None,
            task_description: Some(query.to_string()),
            target_entity: None,
            keywords,
            data_standard: DataStandard {
                budget: "0 USD".into(),
                ..Default::default()
            },
            user_profile: Default::default(),
        })
    }
}
