// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Sampling-backed implementations of all LLM traits.
//!
//! Each struct wraps a [`SamplingHandle`] and delegates inference to the host
//! AI client via MCP `sampling/createMessage` instead of calling DeepSeek /
//! Gemini APIs directly.

use anyhow::{Context, Result};
use base64::Engine as _;
use data_core::metadata::DatasetMetadata;
use data_core::types::SearchResult;
use data_search::engine::{
    CommentClassificationItem, DatasetSelectionTask, ReviewSentiment, SentimentClassifier,
};
use data_search::intent::{QueryProfile, QueryProfiler};
use data_search::sample_eval::{
    DownloadedSample, LlmSampleJudge, ProxyScreeningReport, SampleJudgeReport, SampleRecord,
    SampleRequirements, SampleRequirementsPlanner, SeedRecordJudge, SeedRecordJudgeReport,
    SeedRecordScore,
};

use crate::sampling::{SamplingContent, SamplingHandle, SamplingMessage, SamplingRequest};

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// 1. Intent parsing (QueryProfiler)
// ---------------------------------------------------------------------------

pub struct SamplingIntentParser {
    handle: SamplingHandle,
}

impl SamplingIntentParser {
    pub fn new(handle: SamplingHandle) -> Self {
        Self { handle }
    }
}

#[async_trait::async_trait]
impl QueryProfiler for SamplingIntentParser {
    async fn profile(&self, query: &str) -> Result<QueryProfile> {
        let system = data_search::intent::INTENT_SYSTEM_PROMPT;
        let user = data_search::intent::build_intent_user_prompt(query)?;
        let text = self.handle.chat_text(system, user, 256).await?;
        data_search::intent::parse_intent_response(query, &text)
    }
}

// ---------------------------------------------------------------------------
// 2. Sentiment classification
// ---------------------------------------------------------------------------

pub struct SamplingSentimentClassifier {
    handle: SamplingHandle,
}

impl SamplingSentimentClassifier {
    pub fn new(handle: SamplingHandle) -> Self {
        Self { handle }
    }
}

#[async_trait::async_trait]
impl SentimentClassifier for SamplingSentimentClassifier {
    async fn classify(
        &self,
        items: &[CommentClassificationItem],
    ) -> Result<HashMap<usize, ReviewSentiment>> {
        let system = "You classify buyer review comments for dataset marketplace transactions and must return strict JSON.";
        let items_json = serde_json::to_string(items)?;
        let prompt = format!(
            "Classify each buyer review comment about a dataset purchase. \
             Return JSON only with this exact schema: \
             {{\"items\":[{{\"id\":0,\"sentiment\":\"positive|neutral|negative\"}}]}}. \
             Use the comment as the primary signal and the numeric rating only as a weak hint. \
             Mark complaints about poor quality, missing data, scams, bad seller behavior, \
             or mismatched content as negative. Mark praise, satisfaction, successful delivery, \
             or recommendation as positive. Mark factual, mixed, or unclear comments as neutral.\n\n\
             Reviews:\n{items_json}"
        );
        let payload: data_search::engine::DeepSeekSentimentPayload =
            self.handle.chat_json(system, prompt, 512).await?;
        Ok(payload
            .items
            .into_iter()
            .filter_map(|item| {
                data_search::engine::parse_review_sentiment(&item.sentiment).map(|s| (item.id, s))
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// 3. Sample judge (LlmSampleJudge)
// ---------------------------------------------------------------------------

pub struct SamplingSampleJudge {
    handle: SamplingHandle,
    max_preview_records: usize,
}

impl SamplingSampleJudge {
    pub fn new(handle: SamplingHandle) -> Self {
        Self {
            handle,
            max_preview_records: 8,
        }
    }
}

#[async_trait::async_trait]
impl LlmSampleJudge for SamplingSampleJudge {
    async fn judge_sample(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        task: &DatasetSelectionTask,
        requirements: &SampleRequirements,
        sample: &DownloadedSample,
        screening: &ProxyScreeningReport,
    ) -> Result<SampleJudgeReport> {
        let system = data_search::sample_eval::SAMPLE_JUDGE_SYSTEM_PROMPT;
        let user = data_search::sample_eval::build_sample_judge_user_prompt(
            result,
            metadata,
            task,
            requirements,
            sample,
            screening,
            self.max_preview_records,
        )?;
        self.handle.chat_json(system, user, 384).await
    }
}

// ---------------------------------------------------------------------------
// 4. Seed record judge (SeedRecordJudge) — text
// ---------------------------------------------------------------------------

pub struct SamplingSeedRecordJudge {
    handle: SamplingHandle,
    max_record_chars: usize,
}

impl SamplingSeedRecordJudge {
    pub fn new(handle: SamplingHandle) -> Self {
        Self {
            handle,
            max_record_chars: 800,
        }
    }
}

#[async_trait::async_trait]
impl SeedRecordJudge for SamplingSeedRecordJudge {
    async fn judge_records(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        task: &DatasetSelectionTask,
        sample: &DownloadedSample,
        records: &[SampleRecord],
    ) -> Result<SeedRecordJudgeReport> {
        let system = data_search::sample_eval::SEED_RECORD_JUDGE_SYSTEM_PROMPT;
        let user = data_search::sample_eval::build_seed_record_judge_user_prompt(
            result,
            metadata,
            task,
            sample,
            records,
            self.max_record_chars,
        )?;
        self.handle.chat_json(system, user, 640).await
    }
}

// ---------------------------------------------------------------------------
// 5. Seed record judge — image (multimodal via sampling)
// ---------------------------------------------------------------------------

pub struct SamplingImageSeedRecordJudge {
    handle: SamplingHandle,
    max_record_chars: usize,
}

impl SamplingImageSeedRecordJudge {
    pub fn new(handle: SamplingHandle) -> Self {
        Self {
            handle,
            max_record_chars: 800,
        }
    }
}

#[async_trait::async_trait]
impl SeedRecordJudge for SamplingImageSeedRecordJudge {
    async fn judge_records(
        &self,
        result: &SearchResult,
        metadata: &DatasetMetadata,
        task: &DatasetSelectionTask,
        _sample: &DownloadedSample,
        records: &[SampleRecord],
    ) -> Result<SeedRecordJudgeReport> {
        let mut scored_records = Vec::with_capacity(records.len());
        for record in records {
            let image_path = data_search::sample_eval::sample_record_image_path(record)
                .ok_or_else(|| {
                    anyhow::anyhow!("sample record {} missing local_image_path", record.id)
                })?;
            let image_bytes = std::fs::read(&image_path)
                .with_context(|| format!("read sample image {}", image_path.display()))?;
            let mime_type = record
                .metadata
                .get("local_image_mime_type")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    data_search::sample_eval::guess_image_mime_type(&image_path).to_string()
                });

            let task_text = data_search::sample_eval::build_gemini_image_eval_task(
                result,
                metadata,
                task,
                record,
                self.max_record_chars,
            );
            let base64_data = base64::engine::general_purpose::STANDARD.encode(&image_bytes);

            let req = SamplingRequest {
                messages: vec![
                    SamplingMessage {
                        role: "user".into(),
                        content: SamplingContent::text(&task_text),
                    },
                    SamplingMessage {
                        role: "user".into(),
                        content: SamplingContent::image(base64_data, mime_type),
                    },
                ],
                system_prompt: Some(
                    "You evaluate image samples from candidate datasets. \
                     Return a numeric score 0-100 indicating how useful this image is for the task."
                        .into(),
                ),
                max_tokens: 128,
            };

            let resp = self.handle.create_message(req).await?;
            let text = resp.content.text.unwrap_or_default();
            let utility_score =
                data_search::sample_eval::parse_gemini_numeric_result_from_text(&text)
                    .unwrap_or(50.0)
                    .clamp(0.0, 100.0);

            scored_records.push(SeedRecordScore {
                record_id: record.id.clone(),
                utility_score,
                rationale: if text.trim().is_empty() {
                    format!("host sampling image score {utility_score:.1}")
                } else {
                    text
                },
            });
        }

        Ok(SeedRecordJudgeReport {
            summary: format!(
                "scored {} image seed records via host MCP sampling",
                scored_records.len()
            ),
            scored_records,
        })
    }
}

// ---------------------------------------------------------------------------
// 6. Sample requirements planner (SampleRequirementsPlanner)
// ---------------------------------------------------------------------------

pub struct SamplingRequirementsPlanner {
    handle: SamplingHandle,
}

impl SamplingRequirementsPlanner {
    pub fn new(handle: SamplingHandle) -> Self {
        Self { handle }
    }
}

#[async_trait::async_trait]
impl SampleRequirementsPlanner for SamplingRequirementsPlanner {
    async fn plan_requirements(&self, task: &DatasetSelectionTask) -> Result<SampleRequirements> {
        let system = data_search::sample_eval::REQUIREMENTS_SYSTEM_PROMPT;
        let user = data_search::sample_eval::build_requirements_user_prompt(task);
        self.handle.chat_json(system, user, 256).await
    }
}
