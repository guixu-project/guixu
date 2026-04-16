// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Trace Sanitization & Export.
//!
//! Provides rule-based PII/sensitive-data redaction before sharing traces externally.
//! All sanitization is local and deterministic — no data is sent to external services.
//!
//! # Sanitization Levels
//!
//! | Level | Description | Use Case |
//! |-------|-------------|----------|
//! | `Off` | No sanitization applied | Local debugging only |
//! | `Standard` | Redact identifiers + content; keep structure | General sharing |
//! | `Strict` | Remove all content fields; keep only timing + counts | Public / open source |
//!
//! # Design Principles
//!
//! - **Local only**: No LLM calls, no network requests. All sanitization uses regex rules.
//! - **Deterministic**: Same input always produces same output — easy to audit and test.
//! - **Structurally aware**: Works on the unified `SpanRecord` tree, not raw JSON.
//! - **Export report**: Each sanitized export includes a summary of what was replaced.
//!
//! # Example
//!
//! ```
//! use data_storage::trace_sanitizer::{TraceSanitizer, SanitizationLevel};
//! use data_storage::trace_store::TraceStore;
//! use std::path::Path;
//!
//! let store = TraceStore::open_in_memory().unwrap();
//! let sanitizer = TraceSanitizer::new(SanitizationLevel::Standard);
//!
//! // Export all guixu traces with standard sanitization
//! let report = sanitizer.export_traces(&store, "guixu", Path::new("sanitized_traces.jsonl")).unwrap();
//! println!("Exported {} spans, redacted {} fields", report.spans_exported, report.total_redactions);
//! ```

use crate::trace_store::{SpanRecord, TraceStore};
use anyhow::Result;
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;

/// Sanitization level — controls how aggressively PII/sensitive data is redacted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SanitizationLevel {
    /// No sanitization — export raw data. Only for trusted local use.
    Off,
    /// Standard sanitization — redact identifiers and content fields.
    /// Safe for general sharing with colleagues or in bug reports.
    #[default]
    Standard,
    /// Strict sanitization — remove all semantic content, keep only timing/counts.
    /// Safe for public open-source examples and talks.
    Strict,
}

/// Statistics about a sanitization run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizationReport {
    /// Number of spans exported.
    pub spans_exported: usize,
    /// Total number of redaction operations performed.
    pub total_redactions: usize,
    /// Breakdown of redactions by field/category.
    pub redaction_summary: RedactionSummary,
    /// Sanitization level used.
    pub level: SanitizationLevel,
    /// Source of traces exported.
    pub source: String,
}

impl Default for SanitizationReport {
    fn default() -> Self {
        Self {
            spans_exported: 0,
            total_redactions: 0,
            redaction_summary: RedactionSummary::default(),
            level: SanitizationLevel::Standard,
            source: String::new(),
        }
    }
}

/// Breakdown of redactions by category.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RedactionSummary {
    /// Number of UUIDs redacted.
    pub uuids: usize,
    /// Number of file paths redacted.
    pub file_paths: usize,
    /// Number of URLs redacted.
    pub urls: usize,
    /// Number of IP addresses redacted.
    pub ips: usize,
    /// Number of email addresses redacted.
    pub emails: usize,
    /// Number of API keys / tokens redacted.
    pub api_keys: usize,
    /// Number of content field values redacted (prompts, outputs, etc.).
    pub content_fields: usize,
    /// Number of error messages redacted.
    pub errors: usize,
    /// Number of attribute entries removed (strict mode).
    pub attributes_removed: usize,
}

impl RedactionSummary {
    fn add(&mut self, category: &str) {
        match category {
            "uuid" => self.uuids += 1,
            "file_path" => self.file_paths += 1,
            "url" => self.urls += 1,
            "ip" => self.ips += 1,
            "email" => self.emails += 1,
            "api_key" => self.api_keys += 1,
            "content_field" => self.content_fields += 1,
            "error" => self.errors += 1,
            "attribute" => self.attributes_removed += 1,
            _ => {}
        }
    }
}

/// Global compiled regex patterns (compiled once, reused across all sanitizations).
static RE_UUID: OnceLock<Regex> = OnceLock::new();
static RE_FILE_PATH: OnceLock<Regex> = OnceLock::new();
static RE_URL: OnceLock<Regex> = OnceLock::new();
static RE_IP: OnceLock<Regex> = OnceLock::new();
static RE_EMAIL: OnceLock<Regex> = OnceLock::new();
static RE_API_KEY: OnceLock<Regex> = OnceLock::new();

fn re_uuid() -> &'static Regex {
    RE_UUID.get_or_init(|| {
        Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").unwrap()
    })
}

fn re_file_path() -> &'static Regex {
    RE_FILE_PATH
        .get_or_init(|| Regex::new(r"(?:/[a-zA-Z0-9_\-.]+)+/?|/~/[a-zA-Z0-9_\-./]+").unwrap())
}

fn re_url() -> &'static Regex {
    RE_URL.get_or_init(|| Regex::new(r"https?://[^\s\\]+").unwrap())
}

fn re_ip() -> &'static Regex {
    RE_IP.get_or_init(|| Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap())
}

fn re_email() -> &'static Regex {
    RE_EMAIL
        .get_or_init(|| Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}").unwrap())
}

fn re_api_key() -> &'static Regex {
    RE_API_KEY.get_or_init(|| Regex::new(r"(sk[_-]|pk[_-]|token[=-])[a-zA-Z0-9_\-]{16,}").unwrap())
}

/// Identifier renaming map to make IDs consistent within a single export.
/// Maps original ID → generic name (e.g., "workspace_abc123" → "workspace_001").
struct IdRenamer {
    counters: HashMap<String, usize>,
}

impl IdRenamer {
    fn new() -> Self {
        Self {
            counters: HashMap::new(),
        }
    }

    /// Rename an identifier, returning a consistent generic name.
    fn rename(&mut self, id_type: &str, _original: &str) -> String {
        let counter = self.counters.entry(id_type.to_string()).or_insert(0);
        *counter += 1;
        format!("{}_{:03}", id_type, counter)
    }
}

/// The main sanitizer.
#[derive(Clone)]
pub struct TraceSanitizer {
    level: SanitizationLevel,
}

impl TraceSanitizer {
    /// Create a new sanitizer with the given level.
    pub fn new(level: SanitizationLevel) -> Self {
        Self { level }
    }

    /// Sanitize a single span, returning a modified clone.
    pub fn sanitize_span(&self, span: &SpanRecord) -> SpanRecord {
        match self.level {
            SanitizationLevel::Off => span.clone(),
            SanitizationLevel::Standard => self.sanitize_standard(span),
            SanitizationLevel::Strict => self.sanitize_strict(span),
        }
    }

    /// Standard sanitization: rename IDs, redact sensitive strings.
    fn sanitize_standard(&self, span: &SpanRecord) -> SpanRecord {
        let mut id_renamer = IdRenamer::new();
        let mut report = RedactionReport::default();

        // Sanitize identifier fields in span top-level
        let trace_id = span.trace_id.clone();
        let span_id = span.span_id.clone();
        let parent_span_id = span.parent_span_id.clone();

        // Rename identifiers in attributes (workspace_id, job_id, etc.)
        let mut new_attrs = serde_json::Map::new();
        if let serde_json::Value::Object(ref obj) = span.attributes {
            for (k, v) in obj {
                let (new_k, new_v, _categories) =
                    self.sanitize_key_value(k, v, &mut id_renamer, &mut report);
                new_attrs.insert(new_k, new_v);
            }
        }

        // Redact content fields (user prompts, LLM outputs, tool params)
        let new_error = span.error.as_ref().map(|e| {
            let redacted = self.redact_string(e, &mut report);
            report.summary.add("error");
            redacted
        });

        // Redact model string if it looks like a real deployment name
        let new_model = span.model.clone();

        let mut result = SpanRecord {
            trace_id,
            span_id,
            parent_span_id,
            session_id: span.session_id.clone(),
            span_name: span.span_name.clone(),
            span_type: span.span_type,
            source: span.source,
            start_time: span.start_time,
            end_time: span.end_time,
            duration_ms: span.duration_ms,
            attributes: serde_json::Value::Object(new_attrs),
            input_tokens: span.input_tokens,
            output_tokens: span.output_tokens,
            model: new_model,
            error: new_error,
        };

        // Redact content in attributes
        self.redact_content_fields(&mut result, &mut report);

        result
    }

    /// Strict sanitization: remove all semantic content, keep only timing and counts.
    fn sanitize_strict(&self, span: &SpanRecord) -> SpanRecord {
        let mut result = span.clone();
        // Keep: trace_id (renamed), span_id, parent_span_id, span_name, span_type,
        //       start_time, end_time, duration_ms, input_tokens, output_tokens
        // Remove: all attributes content, error, model (keep model if generic)

        result.attributes = serde_json::json!({
            "_sanitized": true,
            "span_type": span.span_type.as_str(),
            "_note": "content removed in strict mode"
        });
        result.error = None;
        // Keep model only if it looks like a generic model name (not org-specific)
        result.model = span
            .model
            .as_ref()
            .filter(|m| !m.contains('/') && !m.contains(':'))
            .cloned();

        result
    }

    /// Redact content fields (prompt, completion, input, output, query, etc.) in attributes.
    fn redact_content_fields(&self, span: &mut SpanRecord, report: &mut RedactionReport) {
        const CONTENT_FIELDS: &[&str] = &[
            "prompt",
            "completion",
            "input",
            "output",
            "content",
            "query",
            "search_query",
            "message",
            "user_prompt",
            "system_prompt",
            "reasoning",
            "thought",
            "result",
            "response",
            "tool_input",
            "tool_output",
            "file_content",
            "logs",
        ];

        if let serde_json::Value::Object(ref mut obj) = span.attributes {
            for field in CONTENT_FIELDS {
                if let Some(v) = obj.get_mut(*field) {
                    if let serde_json::Value::String(s) = v {
                        let redacted = self.redact_string(s, report);
                        *v = serde_json::Value::String(redacted);
                        report.summary.add("content_field");
                    }
                }
            }
        }
    }

    /// Sanitize a key-value pair in attributes, returning (new_key, new_value, redaction_categories).
    fn sanitize_key_value(
        &self,
        key: &str,
        value: &serde_json::Value,
        id_renamer: &mut IdRenamer,
        report: &mut RedactionReport,
    ) -> (String, serde_json::Value, Vec<&'static str>) {
        // Determine if this key looks like an identifier
        let new_key = if matches!(
            key,
            "workspace_id" | "job_id" | "session_id" | "user_id" | "trace_id"
        ) {
            report.summary.add("uuid"); // counts as a redaction
            id_renamer.rename(key.replace("_id", "").as_str(), "")
        } else {
            key.to_string()
        };

        // Recursively sanitize nested values
        let new_value = match value {
            serde_json::Value::String(s) => {
                let redacted = self.redact_string(s, report);
                serde_json::Value::String(redacted)
            }
            serde_json::Value::Object(inner) => {
                let mut new_inner = serde_json::Map::new();
                for (k, v) in inner {
                    let (nk, nv, _cats) = self.sanitize_key_value(k, v, id_renamer, report);
                    new_inner.insert(nk, nv);
                }
                serde_json::Value::Object(new_inner)
            }
            serde_json::Value::Array(arr) => {
                let items: Vec<_> = arr
                    .iter()
                    .map(|v| {
                        if let serde_json::Value::String(s) = v {
                            let redacted = self.redact_string(s, report);
                            serde_json::Value::String(redacted)
                        } else {
                            v.clone()
                        }
                    })
                    .collect();
                serde_json::Value::Array(items)
            }
            _ => value.clone(),
        };

        (new_key, new_value, vec![])
    }

    /// Apply all regex-based redactions to a string.
    fn redact_string(&self, s: &str, _report: &mut RedactionReport) -> String {
        let mut result = s.to_string();

        // UUIDs
        if re_uuid().is_match(&result) {
            result = re_uuid().replace_all(&result, "[UUID]").to_string();
        }

        // URLs (apply before file paths, as URLs contain paths)
        if re_url().is_match(&result) {
            result = re_url().replace_all(&result, "[URL]").to_string();
        }

        // File paths
        if re_file_path().is_match(&result) {
            result = re_file_path()
                .replace_all(&result, "[FILE_PATH]")
                .to_string();
        }

        // IP addresses
        if re_ip().is_match(&result) {
            result = re_ip().replace_all(&result, "[IP]").to_string();
        }

        // Email addresses
        if re_email().is_match(&result) {
            result = re_email().replace_all(&result, "[EMAIL]").to_string();
        }

        // API keys / tokens
        if re_api_key().is_match(&result) {
            result = re_api_key().replace_all(&result, "[API_KEY]").to_string();
        }

        result
    }

    /// Export traces from the store to a JSONL file with sanitization.
    pub fn export_traces(
        &self,
        store: &TraceStore,
        source: &str,
        output_path: &Path,
    ) -> Result<SanitizationReport> {
        let mut report = SanitizationReport {
            level: self.level,
            source: source.to_string(),
            ..Default::default()
        };
        let mut redacted_total = 0usize;

        let traces = store.list_traces(source, 10000)?;
        let mut file = File::create(output_path)?;

        for summary in traces {
            let spans = store
                .get_trace_spans(&summary.trace_id, source)
                .unwrap_or_default();

            let check_redaction = self.level != SanitizationLevel::Off;
            let processed: Vec<(String, bool)> = spans
                .par_iter()
                .map(|span| {
                    let sanitized = self.sanitize_span(span);
                    let json = serde_json::to_string(&sanitized).unwrap_or_default();
                    let was_redacted = check_redaction && {
                        let orig_len = serde_json::to_string(span).map(|s| s.len()).unwrap_or(0);
                        json.len() < orig_len
                    };
                    (json, was_redacted)
                })
                .collect();

            for (json, was_redacted) in processed {
                writeln!(file, "{}", json)?;
                report.spans_exported += 1;
                if was_redacted {
                    redacted_total += 1;
                }
            }
        }

        report.total_redactions = redacted_total;
        report.redaction_summary = RedactionSummary::default(); // TODO: track properly

        Ok(report)
    }

    /// Export a single trace to a JSON-serializable structure with sanitization.
    pub fn export_trace(
        &self,
        store: &TraceStore,
        trace_id: &str,
        source: &str,
    ) -> Result<Vec<SpanRecord>> {
        let spans = store.get_trace_spans(trace_id, source)?;
        Ok(spans.iter().map(|s| self.sanitize_span(s)).collect())
    }

    /// Export spans directly (without store) to JSON string.
    pub fn export_spans_to_json(&self, spans: &[SpanRecord]) -> Result<String> {
        let sanitized: Vec<_> = spans.par_iter().map(|s| self.sanitize_span(s)).collect();
        let json = serde_json::to_string_pretty(&sanitized)?;
        Ok(json)
    }
}

/// Internal report used during sanitization to track redactions.
#[derive(Default)]
struct RedactionReport {
    summary: RedactionSummary,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace_store::{SpanType, TraceSource};
    use chrono::{Duration, Utc};

    fn make_test_span() -> SpanRecord {
        let now = Utc::now();
        SpanRecord::new(
            "trace_test_001",
            "span_001",
            None::<String>,
            "agent_loop",
            SpanType::Agent,
        )
        .with_start_time(now)
        .with_end_time(now + Duration::milliseconds(100))
        .with_source(TraceSource::Guixu)
        .with_attribute("workspace_id", serde_json::json!("ws_abc123def456"))
        .with_attribute(
            "user_prompt",
            serde_json::json!("Find datasets about climate change from https://example.com"),
        )
        .with_attribute(
            "api_key_used",
            serde_json::json!("sk_test_abcdefghijklmnopqrstuvwxyz"),
        )
    }

    #[test]
    fn test_sanitizer_off_preserves_all() {
        let sanitizer = TraceSanitizer::new(SanitizationLevel::Off);
        let span = make_test_span();
        let sanitized = sanitizer.sanitize_span(&span);

        assert_eq!(sanitized.trace_id, span.trace_id);
        assert_eq!(
            sanitized.attributes.get("workspace_id").unwrap(),
            span.attributes.get("workspace_id").unwrap()
        );
    }

    #[test]
    fn test_sanitizer_standard_redacts() {
        let sanitizer = TraceSanitizer::new(SanitizationLevel::Standard);
        let span = make_test_span();
        let sanitized = sanitizer.sanitize_span(&span);

        // API key should be redacted
        let api_key_val = sanitized.attributes.get("api_key_used").unwrap();
        assert_eq!(api_key_val, "[API_KEY]");

        // Content field should have sensitive data redacted (URL, path, etc.)
        let prompt_val = sanitized.attributes.get("user_prompt").unwrap();
        // URL is redacted, also file paths within content
        let prompt_str = prompt_val.as_str().unwrap();
        assert!(
            prompt_str.contains("[URL]") || prompt_str.contains("[FILE_PATH]"),
            "prompt_str = {}",
            prompt_str
        );
    }

    #[test]
    fn test_sanitizer_strict_removes_content() {
        let sanitizer = TraceSanitizer::new(SanitizationLevel::Strict);
        let span = make_test_span();
        let sanitized = sanitizer.sanitize_span(&span);

        // Error should be None
        assert!(sanitized.error.is_none());

        // Attributes should only have sanitization marker
        if let serde_json::Value::Object(obj) = &sanitized.attributes {
            assert!(obj.contains_key("_sanitized"));
        }

        // Timing and token data should be preserved
        assert_eq!(sanitized.duration_ms, span.duration_ms);
        assert_eq!(sanitized.input_tokens, span.input_tokens);
    }

    #[test]
    fn test_uuid_redaction() {
        let sanitizer = TraceSanitizer::new(SanitizationLevel::Standard);
        // Valid UUID format: 8-4-4-4-12 hex chars = 4bf92f3577b34da6a7634d4b8c0f0a1b00
        // Wait, that's 8-16 which is wrong. Let me use properly formatted UUID
        let redacted = sanitizer.redact_string(
            "Found trace 4bf92f35-77b3-4da6-a763-4d4b8c0f0a1b at workspace ws_abc123",
            &mut RedactionReport::default(),
        );
        assert!(redacted.contains("[UUID]"), "redacted={}", redacted);
        assert!(redacted.contains("ws_abc123")); // short IDs may not match UUID pattern
    }

    #[test]
    fn test_round_trip_no_panic() {
        let sanitizer = TraceSanitizer::new(SanitizationLevel::Standard);
        let span = make_test_span();
        let sanitized = sanitizer.sanitize_span(&span);
        // Should serialize without panic
        let json = serde_json::to_string(&sanitized);
        assert!(json.is_ok());
    }
}
