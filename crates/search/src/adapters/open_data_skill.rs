// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use data_core::types::*;
use serde::Deserialize;
use tracing::warn;

use super::util::infer_data_type_from_title;
use super::ExternalAdapter;

const SKILLS_API: &str = "https://www.guixu.org/api/open-data-skills";
const CACHE_TTL: Duration = Duration::from_secs(300); // 5 min

#[derive(Debug, Clone, Deserialize)]
struct SkillsResponse {
    skills: Vec<Skill>,
    #[serde(default)]
    #[allow(dead_code)]
    total: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct Skill {
    id: String,
    platform_name: String,
    description: String,
    base_url: String,
    endpoints: Vec<SkillEndpoint>,
    tags: Vec<String>,
    #[serde(default)]
    download_count: u64,
    #[serde(default)]
    health_status: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SkillEndpoint {
    path: String,
    #[serde(default = "default_method")]
    #[allow(dead_code)]
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

fn default_method() -> String {
    "GET".into()
}

struct SkillCache {
    skills: Vec<Skill>,
    fetched_at: Instant,
}

pub struct OpenDataSkillAdapter {
    client: reqwest::Client,
    skills_api: String,
    cache: Mutex<Option<SkillCache>>,
}

impl Default for OpenDataSkillAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_default(),
            skills_api: std::env::var("GUIXU_SKILLS_API").unwrap_or_else(|_| SKILLS_API.into()),
            cache: Mutex::new(None),
        }
    }
}

impl OpenDataSkillAdapter {
    /// Fetch skills with in-memory cache (TTL = 5 min).
    async fn fetch_skills(&self) -> Result<Vec<Skill>> {
        // Check cache
        if let Ok(guard) = self.cache.lock() {
            if let Some(ref c) = *guard {
                if c.fetched_at.elapsed() < CACHE_TTL {
                    return Ok(c.skills.clone());
                }
            }
        }

        let resp: SkillsResponse = tokio::time::timeout(
            Duration::from_secs(10),
            self.client.get(&self.skills_api).send(),
        )
        .await
        .map_err(|_| anyhow!("open_data_skill: timeout fetching skill list"))??
        .error_for_status()?
        .json()
        .await?;

        // Only use verified/unverified skills, skip broken
        let skills: Vec<Skill> = resp
            .skills
            .into_iter()
            .filter(|s| s.health_status != "broken")
            .collect();

        if let Ok(mut guard) = self.cache.lock() {
            *guard = Some(SkillCache {
                skills: skills.clone(),
                fetched_at: Instant::now(),
            });
        }

        Ok(skills)
    }

    /// Call a skill's first endpoint with the user query and parse results.
    async fn call_skill(
        &self,
        skill: &Skill,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let ep = skill
            .endpoints
            .first()
            .ok_or_else(|| anyhow!("no endpoints"))?;
        let url = format!("{}{}", skill.base_url.trim_end_matches('/'), ep.path);

        let mut params: Vec<(String, String)> = Vec::new();
        if let Some(obj) = ep.params.as_object() {
            for (k, v) in obj {
                let val = v.as_str().unwrap_or("").to_string();
                if val == "query" || val == "{query}" {
                    params.push((k.clone(), query.to_string()));
                } else {
                    params.push((k.clone(), val));
                }
            }
        }
        if !params.iter().any(|(_, v)| v == query) {
            params.push(("search".into(), query.into()));
        }

        let resp = tokio::time::timeout(
            Duration::from_secs(10),
            self.client.get(&url).query(&params).send(),
        )
        .await
        .map_err(|_| anyhow!("open_data_skill({}): timeout", skill.platform_name))??
        .error_for_status()?
        .text()
        .await?;

        let items: Vec<serde_json::Value> = serde_json::from_str(&resp).unwrap_or_else(|_| {
            let obj: serde_json::Value = serde_json::from_str(&resp).unwrap_or_default();
            for key in &["data", "results", "items", "datasets", "records"] {
                if let Some(arr) = obj.get(key).and_then(|v| v.as_array()) {
                    return arr.clone();
                }
            }
            vec![]
        });

        Ok(items
            .iter()
            .take(limit)
            .enumerate()
            .map(|(i, item)| {
                let title = item
                    .get("title")
                    .or_else(|| item.get("name"))
                    .or_else(|| item.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&skill.platform_name)
                    .to_string();
                let desc = item
                    .get("description")
                    .or_else(|| item.get("abstract"))
                    .and_then(|v| v.as_str())
                    .map(String::from);

                SearchResult {
                    cid: DatasetCid(format!("ods:{}:{}", skill.id, i)),
                    title: title.clone(),
                    description: desc,
                    tags: skill.tags.clone(),
                    schema: DatasetSchema {
                        columns: vec![],
                        row_count: 0,
                        size_bytes: 0,
                    },
                    quality: None,
                    price: Price::free(),
                    license: License {
                        spdx_id: "unknown".into(),
                        commercial_use: false,
                        derivative_allowed: false,
                    },
                    provider: Did(format!("ods:{}", skill.platform_name)),
                    source: DataSource::OpenDataSkill,
                    market: Some(DatasetMarketStats {
                        download_count: skill.download_count,
                        review_count: 0,
                        trade_count: 0,
                    }),
                    data_type: infer_data_type_from_title(&title),
                    created_at: chrono::Utc::now(),
                    seller_endpoint: Some(skill.base_url.clone()),
                    source_attributes: None,
                }
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for OpenDataSkillAdapter {
    fn name(&self) -> &str {
        "open_data_skill"
    }
    fn source_type(&self) -> DataSource {
        DataSource::OpenDataSkill
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let skills = self.fetch_skills().await.unwrap_or_default();
        if skills.is_empty() {
            return Ok(vec![]);
        }

        let q = query.to_lowercase();
        let relevant: Vec<&Skill> = skills
            .iter()
            .filter(|s| {
                s.platform_name.to_lowercase().contains(&q)
                    || s.description.to_lowercase().contains(&q)
                    || s.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect();

        let targets: Vec<&Skill> = if relevant.is_empty() {
            skills.iter().collect()
        } else {
            relevant
        };

        let per_skill = (limit / targets.len().max(1)).max(3);
        let capped: Vec<&Skill> = targets.into_iter().take(5).collect();

        // Concurrent calls
        let futures: Vec<_> = capped
            .iter()
            .map(|skill| self.call_skill(skill, query, per_skill))
            .collect();
        let outcomes = futures::future::join_all(futures).await;

        let mut results = Vec::new();
        for (i, outcome) in outcomes.into_iter().enumerate() {
            match outcome {
                Ok(mut r) => results.append(&mut r),
                Err(e) => {
                    warn!(
                        skill = capped[i].platform_name,
                        error = %e,
                        "open_data_skill: call failed"
                    );
                }
            }
        }
        results.truncate(limit);
        Ok(results)
    }
}
