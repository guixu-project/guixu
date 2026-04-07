// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::time::Duration;

use anyhow::{anyhow, Result};
use data_core::types::*;
use serde_json::json;
use tracing::{debug, warn};

use super::util::infer_data_type_from_title;
use super::ExternalAdapter;

/// Supported cloud-drive platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanPlatform {
    Baidu,
    Quark,
    Aliyun,
    OneDrive,
    Xunlei,
    _115,
    Other,
}

impl PanPlatform {
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "baidu" => Self::Baidu,
            "quark" => Self::Quark,
            "aliyun" | "alipan" => Self::Aliyun,
            "onedrive" => Self::OneDrive,
            "xunlei" | "thunder" => Self::Xunlei,
            "115" | "_115" => Self::_115,
            _ => Self::Other,
        }
    }

    /// Detect platform from a share URL when the type field is missing or "other".
    fn from_url(url: &str) -> Self {
        if url.contains("pan.baidu.com") {
            Self::Baidu
        } else if url.contains("pan.quark.cn") {
            Self::Quark
        } else if url.contains("alipan.com") || url.contains("aliyundrive.com") {
            Self::Aliyun
        } else if url.contains("pan.xunlei.com") {
            Self::Xunlei
        } else if url.contains("1drv.ms") || url.contains("onedrive") {
            Self::OneDrive
        } else if url.contains("115.com") {
            Self::_115
        } else {
            Self::Other
        }
    }

    /// Resolve platform: prefer explicit type, fall back to URL detection.
    fn resolve(type_hint: &str, url: &str) -> Self {
        let from_type = Self::from_str(type_hint);
        if from_type == Self::Other {
            Self::from_url(url)
        } else {
            from_type
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Baidu => "baidu",
            Self::Quark => "quark",
            Self::Aliyun => "aliyun",
            Self::OneDrive => "onedrive",
            Self::Xunlei => "xunlei",
            Self::_115 => "115",
            Self::Other => "other",
        }
    }
}

/// A normalized pan-search result after dedup and resolution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PanResource {
    pub platform: String,
    pub share_url: String,
    pub access_code: Option<String>,
    pub title: String,
    pub note: Option<String>,
    pub source: String,
    pub datetime: Option<String>,
    pub is_alive: Option<bool>,
}

/// Clean TG-style titles: extract the first meaningful line, strip emoji spam,
/// and truncate to a reasonable length.
fn clean_title(raw: &str) -> String {
    // Take text before the first "·" or "📜" or "💾" separator (TG post structure)
    let cut = raw
        .find('·')
        .or_else(|| raw.find("📜"))
        .or_else(|| raw.find("💾"))
        .or_else(|| raw.find("📁"))
        .or_else(|| raw.find("🏷"))
        .unwrap_or(raw.len());
    let mut title: String = raw[..cut].trim().to_string();

    // Strip leading category-style prefixes such as "#Category🗄 "
    if let Some(pos) = title.find('🗄') {
        let after = &title[pos + '🗄'.len_utf8()..].trim_start();
        if !after.is_empty() {
            title = after.to_string();
        }
    } else if title.starts_with('#') {
        // Strip "#tag " prefix
        if let Some(pos) = title.find(' ') {
            title = title[pos..].trim_start().to_string();
        }
    }

    // Truncate at 120 chars
    if title.chars().count() > 120 {
        title = title.chars().take(120).collect::<String>() + "…";
    }

    if title.is_empty() {
        raw.chars().take(80).collect()
    } else {
        title
    }
}

/// Resolve share links: extract access code from URL query params if not
/// already provided, and check liveness via a lightweight HEAD request.
struct ShareLinkResolver {
    client: reqwest::Client,
}

impl ShareLinkResolver {
    fn new(client: &reqwest::Client) -> Self {
        Self {
            client: client.clone(),
        }
    }

    /// Extract access code embedded in the URL (e.g. `?pwd=xxxx`).
    fn extract_code_from_url(url: &str) -> Option<String> {
        // Handles ?pwd=xxxx and #pwd=xxxx patterns
        let query_start = url.find('?').or_else(|| url.find('#'))?;
        let query = &url[query_start + 1..];
        for pair in query.split('&') {
            if let Some(val) = pair.strip_prefix("pwd=") {
                let val = val.trim_end_matches('#');
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
        None
    }

    /// Batch-resolve a list of resources concurrently (max 10 at a time).
    async fn resolve_batch(&self, resources: &mut [PanResource]) {
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(10));
        let mut handles = Vec::new();

        for r in resources.iter_mut() {
            // Always do sync parts immediately
            if r.access_code.is_none() {
                r.access_code = Self::extract_code_from_url(&r.share_url);
            }
            r.title = clean_title(&r.title);
        }

        // Async liveness checks
        let client = self.client.clone();
        for (i, r) in resources.iter().enumerate() {
            let url = r.share_url.clone();
            let sem = semaphore.clone();
            let c = client.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;
                let alive =
                    match tokio::time::timeout(Duration::from_secs(5), c.head(&url).send()).await {
                        Ok(Ok(resp)) => {
                            let s = resp.status().as_u16();
                            if s == 200 || (300..400).contains(&s) {
                                Some(true)
                            } else if s == 404 {
                                Some(false)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                (i, alive)
            }));
        }

        for handle in handles {
            if let Ok((i, alive)) = handle.await {
                resources[i].is_alive = alive;
            }
        }
    }
}

/// Adapter that queries self-hosted PanSou instances for public cloud-drive
/// share links (Baidu, Quark, Aliyun, etc.).
pub struct PanSearchAdapter {
    client: reqwest::Client,
    /// PanSou base URL, e.g. `https://so.example.com`.
    pansou_base_url: String,
    /// Optional YzPanSearch base URL as fallback.
    yz_base_url: Option<String>,
}

impl Default for PanSearchAdapter {
    fn default() -> Self {
        let base =
            std::env::var("GUIXU_PANSOU_URL").unwrap_or_else(|_| "https://so.252035.xyz".into());
        let yz = std::env::var("GUIXU_YZ_PANSEARCH_URL").ok();
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(15))
                .user_agent("guixu/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            pansou_base_url: base,
            yz_base_url: yz,
        }
    }
}

impl PanSearchAdapter {
    // ── PanSou backend ──────────────────────────────────────────────────

    async fn search_pansou(
        &self,
        query: &str,
        cloud_types: Option<&str>,
        limit: usize,
    ) -> Result<Vec<PanResource>> {
        let url = format!("{}/api/search", self.pansou_base_url);
        let mut body = json!({
            "kw": query,
            "res": "results",
            "src": "all",
        });
        if let Some(ct) = cloud_types {
            body["cloud_types"] = json!(ct);
        }

        let resp: serde_json::Value = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let items = resp
            .pointer("/data/results")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("pansou: missing data.results"))?;

        let mut out = Vec::new();
        for item in items.iter().take(limit * 3) {
            let links = item.get("links").and_then(|v| v.as_array());
            let title = item
                .get("title")
                .or_else(|| item.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let datetime = item
                .get("datetime")
                .and_then(|v| v.as_str())
                .map(String::from);

            if let Some(links) = links {
                for link in links {
                    let url = link.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    if url.is_empty() {
                        continue;
                    }
                    let platform = link.get("type").and_then(|v| v.as_str()).unwrap_or("other");
                    let password = link.get("password").and_then(|v| v.as_str());
                    out.push(PanResource {
                        platform: PanPlatform::resolve(platform, url).as_str().to_string(),
                        share_url: url.to_string(),
                        access_code: password.map(String::from),
                        title: title.clone(),
                        note: item
                            .get("content")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        source: item
                            .get("channel")
                            .and_then(|v| v.as_str())
                            .map(|c| format!("tg:{c}"))
                            .unwrap_or_else(|| "pansou".into()),
                        datetime: datetime.clone(),
                        is_alive: None,
                    });
                }
            }
        }
        Ok(out)
    }

    // ── PanSou merged_by_type fallback ──────────────────────────────────

    async fn search_pansou_merged(
        &self,
        query: &str,
        cloud_types: Option<&str>,
        limit: usize,
    ) -> Result<Vec<PanResource>> {
        let url = format!("{}/api/search", self.pansou_base_url);
        let mut body = json!({
            "kw": query,
            "res": "merged_by_type",
            "src": "all",
        });
        if let Some(ct) = cloud_types {
            body["cloud_types"] = json!(ct);
        }

        let resp: serde_json::Value = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let merged = resp
            .pointer("/data/merged_by_type")
            .and_then(|v| v.as_object())
            .ok_or_else(|| anyhow!("pansou: missing data.merged_by_type"))?;

        let mut out = Vec::new();
        for (platform_key, items) in merged {
            let items = match items.as_array() {
                Some(a) => a,
                None => continue,
            };
            for item in items {
                let url_str = item.get("url").and_then(|v| v.as_str()).unwrap_or("");
                if url_str.is_empty() {
                    continue;
                }
                out.push(PanResource {
                    platform: PanPlatform::resolve(platform_key, url_str)
                        .as_str()
                        .to_string(),
                    share_url: url_str.to_string(),
                    access_code: item
                        .get("password")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    title: item
                        .get("note")
                        .and_then(|v| v.as_str())
                        .unwrap_or(query)
                        .to_string(),
                    note: item.get("note").and_then(|v| v.as_str()).map(String::from),
                    source: item
                        .get("source")
                        .and_then(|v| v.as_str())
                        .unwrap_or("pansou")
                        .to_string(),
                    datetime: item
                        .get("datetime")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    is_alive: None,
                });
                if out.len() >= limit * 3 {
                    break;
                }
            }
        }
        Ok(out)
    }

    // ── YzPanSearch backend ─────────────────────────────────────────────

    async fn search_yz(&self, query: &str, _limit: usize) -> Result<Vec<PanResource>> {
        let base = self
            .yz_base_url
            .as_deref()
            .ok_or_else(|| anyhow!("yz_pansearch not configured"))?;
        let url = format!("{}/api/search", base);
        let resp: serde_json::Value = self
            .client
            .get(&url)
            .query(&[("keyword", query)])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let items = resp
            .get("data")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("yz: missing data array"))?;

        Ok(items
            .iter()
            .filter_map(|item| {
                let url_str = item.get("url").or_else(|| item.get("link"))?.as_str()?;
                let platform = item
                    .get("pan_type")
                    .or_else(|| item.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("other");
                Some(PanResource {
                    platform: PanPlatform::resolve(platform, url_str).as_str().to_string(),
                    share_url: url_str.to_string(),
                    access_code: item.get("pwd").and_then(|v| v.as_str()).map(String::from),
                    title: item
                        .get("title")
                        .or_else(|| item.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    note: item
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    source: "yz_pansearch".into(),
                    datetime: item.get("time").and_then(|v| v.as_str()).map(String::from),
                    is_alive: None,
                })
            })
            .collect())
    }

    // ── Dedup + Rank ────────────────────────────────────────────────────

    fn dedup_and_rank(resources: Vec<PanResource>, limit: usize) -> Vec<PanResource> {
        let mut seen = HashSet::new();
        let mut deduped: Vec<PanResource> = resources
            .into_iter()
            .filter(|r| seen.insert(r.share_url.clone()))
            .collect();

        // Sort: has access_code first, then by datetime descending
        deduped.sort_by(|a, b| {
            let code_a = a.access_code.is_some() as u8;
            let code_b = b.access_code.is_some() as u8;
            code_b
                .cmp(&code_a)
                .then_with(|| b.datetime.cmp(&a.datetime))
        });

        deduped.truncate(limit);
        deduped
    }

    // ── Convert to SearchResult ─────────────────────────────────────────

    fn to_search_result(r: &PanResource) -> SearchResult {
        let created = r
            .datetime
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        let cid_input = format!("{}:{}", r.platform, r.share_url);
        let cid_hash = format!(
            "{:x}",
            <sha2::Sha256 as sha2::Digest>::digest(cid_input.as_bytes())
        );

        SearchResult {
            cid: DatasetCid(cid_hash),
            title: r.title.clone(),
            description: r.note.clone(),
            tags: vec![r.platform.clone()],
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
            provider: Did(format!("pan:{}", r.platform)),
            source: DataSource::PanSearch,
            market: None,
            data_type: infer_data_type_from_title(&r.title),
            created_at: created,
            seller_endpoint: None,
            source_attributes: Some(serde_json::json!({
                "share_url": r.share_url,
                "code": r.access_code,
                "platform": r.platform,
                "origin_source": r.source,
                "validity": r.is_alive,
                "snapshot_time": r.datetime,
            })),
            governance: None,
            provider_meta: None,
        }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for PanSearchAdapter {
    fn name(&self) -> &str {
        "pansearch"
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let mut all = Vec::new();

        // Try PanSou results format first
        match self.search_pansou(query, None, limit).await {
            Ok(r) => {
                debug!(
                    adapter = "pansearch",
                    source = "pansou/results",
                    count = r.len(),
                    "ok"
                );
                all.extend(r);
            }
            Err(e) => {
                warn!(adapter = "pansearch", source = "pansou/results", error = %e, "failed");
                // Fallback to merged_by_type
                match self.search_pansou_merged(query, None, limit).await {
                    Ok(r) => {
                        debug!(
                            adapter = "pansearch",
                            source = "pansou/merged",
                            count = r.len(),
                            "ok"
                        );
                        all.extend(r);
                    }
                    Err(e2) => {
                        warn!(adapter = "pansearch", source = "pansou/merged", error = %e2, "failed");
                    }
                }
            }
        }

        // Try YzPanSearch as supplementary source
        match self.search_yz(query, limit).await {
            Ok(r) => {
                debug!(adapter = "pansearch", source = "yz", count = r.len(), "ok");
                all.extend(r);
            }
            Err(e) => {
                warn!(adapter = "pansearch", source = "yz", error = %e, "skipped");
            }
        }

        if all.is_empty() {
            return Err(anyhow!("all pan-search sources failed"));
        }

        let mut deduped = Self::dedup_and_rank(all, limit);

        // Resolve: clean titles, extract codes from URLs, check liveness
        let resolver = ShareLinkResolver::new(&self.client);
        resolver.resolve_batch(&mut deduped).await;

        // Re-sort after resolution (newly discovered codes may change rank)
        deduped.sort_by(|a, b| {
            // alive > unknown > dead
            let alive_score = |r: &PanResource| match r.is_alive {
                Some(true) => 2u8,
                None => 1,
                Some(false) => 0,
            };
            let code_a = a.access_code.is_some() as u8;
            let code_b = b.access_code.is_some() as u8;
            alive_score(b)
                .cmp(&alive_score(a))
                .then_with(|| code_b.cmp(&code_a))
                .then_with(|| b.datetime.cmp(&a.datetime))
        });

        // Filter out confirmed-dead links
        deduped.retain(|r| r.is_alive != Some(false));

        Ok(deduped.iter().map(Self::to_search_result).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_removes_duplicate_urls() {
        let resources = vec![
            PanResource {
                platform: "baidu".into(),
                share_url: "https://pan.baidu.com/s/abc".into(),
                access_code: Some("1234".into()),
                title: "test".into(),
                note: None,
                source: "tg:ch1".into(),
                datetime: Some("2026-01-01T00:00:00Z".into()),
                is_alive: None,
            },
            PanResource {
                platform: "baidu".into(),
                share_url: "https://pan.baidu.com/s/abc".into(),
                access_code: None,
                title: "test dup".into(),
                note: None,
                source: "tg:ch2".into(),
                datetime: Some("2026-01-02T00:00:00Z".into()),
                is_alive: None,
            },
        ];
        let result = PanSearchAdapter::dedup_and_rank(resources, 10);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].access_code, Some("1234".into()));
    }

    #[test]
    fn rank_prefers_access_code() {
        let resources = vec![
            PanResource {
                platform: "quark".into(),
                share_url: "https://pan.quark.cn/s/no_code".into(),
                access_code: None,
                title: "no code".into(),
                note: None,
                source: "pansou".into(),
                datetime: Some("2026-03-01T00:00:00Z".into()),
                is_alive: None,
            },
            PanResource {
                platform: "quark".into(),
                share_url: "https://pan.quark.cn/s/has_code".into(),
                access_code: Some("abcd".into()),
                title: "has code".into(),
                note: None,
                source: "pansou".into(),
                datetime: Some("2026-01-01T00:00:00Z".into()),
                is_alive: None,
            },
        ];
        let result = PanSearchAdapter::dedup_and_rank(resources, 10);
        assert_eq!(result[0].share_url, "https://pan.quark.cn/s/has_code");
    }

    #[test]
    fn to_search_result_populates_source_attributes() {
        let r = PanResource {
            platform: "baidu".into(),
            share_url: "https://pan.baidu.com/s/xyz".into(),
            access_code: Some("9999".into()),
            title: "年少有为 1080p".into(),
            note: Some("电视剧".into()),
            source: "tg:alipanshare".into(),
            datetime: Some("2026-03-30T10:00:00Z".into()),
            is_alive: None,
        };
        let sr = PanSearchAdapter::to_search_result(&r);
        assert_eq!(sr.source, DataSource::PanSearch);
        assert_eq!(sr.data_type, DataType::Video);
        let attrs = sr.source_attributes.unwrap();
        assert_eq!(attrs["share_url"], "https://pan.baidu.com/s/xyz");
        assert_eq!(attrs["code"], "9999");
        assert_eq!(attrs["platform"], "baidu");
    }

    #[test]
    fn xunlei_detected_from_url() {
        assert_eq!(
            PanPlatform::from_url("https://pan.xunlei.com/s/VOoSg1vv3UVZEh5CzS-teRPhA1?pwd=kp2y#"),
            PanPlatform::Xunlei
        );
        assert_eq!(
            PanPlatform::resolve("other", "https://pan.xunlei.com/s/abc"),
            PanPlatform::Xunlei
        );
        // Explicit type wins over URL
        assert_eq!(
            PanPlatform::resolve("baidu", "https://pan.xunlei.com/s/abc"),
            PanPlatform::Baidu
        );
    }

    #[test]
    fn platform_detected_from_url() {
        assert_eq!(
            PanPlatform::from_url("https://pan.baidu.com/s/xxx"),
            PanPlatform::Baidu
        );
        assert_eq!(
            PanPlatform::from_url("https://pan.quark.cn/s/xxx"),
            PanPlatform::Quark
        );
        assert_eq!(
            PanPlatform::from_url("https://www.alipan.com/s/xxx"),
            PanPlatform::Aliyun
        );
        assert_eq!(
            PanPlatform::from_url("https://www.aliyundrive.com/s/xxx"),
            PanPlatform::Aliyun
        );
        assert_eq!(
            PanPlatform::from_url("https://115.com/s/xxx"),
            PanPlatform::_115
        );
        assert_eq!(
            PanPlatform::from_url("https://example.com/file"),
            PanPlatform::Other
        );
    }

    #[test]
    fn extract_code_from_url_pwd_param() {
        assert_eq!(
            ShareLinkResolver::extract_code_from_url("https://pan.xunlei.com/s/abc?pwd=kp2y#"),
            Some("kp2y".into())
        );
        assert_eq!(
            ShareLinkResolver::extract_code_from_url("https://pan.baidu.com/s/abc?pwd=1234"),
            Some("1234".into())
        );
        assert_eq!(
            ShareLinkResolver::extract_code_from_url("https://pan.quark.cn/s/abc"),
            None
        );
        // Hash-style pwd
        assert_eq!(
            ShareLinkResolver::extract_code_from_url("https://pan.xunlei.com/s/abc#pwd=xy12"),
            Some("xy12".into())
        );
    }

    #[test]
    fn clean_title_strips_tg_noise() {
        let raw =
            "#动漫🗄 九阳武神 (2025) [WEB-4K] [国语中字] [更至22集]·📜介绍：前世叶云飞天资过人...";
        let cleaned = clean_title(raw);
        assert_eq!(cleaned, "九阳武神 (2025) [WEB-4K] [国语中字] [更至22集]");
    }

    #[test]
    fn clean_title_preserves_normal_titles() {
        assert_eq!(
            clean_title("年少有为 (2018) 1080p 全集"),
            "年少有为 (2018) 1080p 全集"
        );
    }

    #[test]
    fn clean_title_truncates_long() {
        let long = "a".repeat(200);
        let cleaned = clean_title(&long);
        assert!(cleaned.chars().count() <= 121); // 120 + "…"
    }
}
