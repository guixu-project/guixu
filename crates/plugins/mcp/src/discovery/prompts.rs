// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use serde_json::json;

use crate::discovery::types::PlatformSubAgentTurnInput;

pub fn build_platform_subagent_system_prompt(skill_id: &str) -> String {
    format!(
        "You are a dataset discovery sub-agent for the {skill_id} platform. \
         You are not the final selector. Your only job is to propose search \
         queries for this single platform and discover candidate datasets. \
         Return valid JSON only with keys query_variants, finish, rationale."
    )
}

pub fn build_platform_subagent_user_prompt(input: &PlatformSubAgentTurnInput) -> String {
    json!({
        "worker_id": input.worker_id,
        "skill_id": input.skill_id,
        "raw_query": input.raw_query,
        "intent": {
            "task_type": input.intent.task_type,
            "task_description": input.intent.task_description,
            "target_entity": input.intent.target_entity,
            "keywords": input.intent.keywords,
            "sample_unit": input.intent.data_standard.sample_unit,
            "budget": input.intent.data_standard.budget,
        },
        "current_result_count": input.current_result_count,
        "max_query_variants": input.max_query_variants,
        "output_schema": {
            "query_variants": ["search phrase"],
            "finish": true,
            "rationale": "brief explanation"
        }
    })
    .to_string()
}
