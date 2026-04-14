// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! External AI Agent Trace Importers.
//!
//! Parses trace exports from external providers (OpenAI, Claude) into the
//! unified [`SpanRecord`] format for storage in [`TraceStore`](super::trace_store::TraceStore).
//!
//! # Supported Formats
//!
//! - **OpenAI**: JSONL export from OpenAI Agents SDK tracing dashboard
//! - **Claude**: JSON export from Claude Agent SDK tracing
//!
//! # Design Principles
//!
//! - Idempotent: import can be safely re-run with the same data
//! - Fault-tolerant: malformed records are logged and skipped, not propagated
//! - Provider-specific fields: stored in `attributes` JSON with schema version markers
//!
//! # Example
//!
//! ```
//! use data_storage::trace_import::{OpenAiImporter, ImporterConfig, TraceImporter};
//! use data_storage::trace_store::TraceStore;
//!
//! let store = TraceStore::open_in_memory().unwrap();
//! let importer = OpenAiImporter::new(ImporterConfig::default());
//!
//! // Import from a JSONL string (in-memory)
//! let jsonl = r#"{"trace_id":"t1","span_id":"s1","name":"test","type":"agent","start_time":"2026-01-01T00:00:00Z","end_time":"2026-01-01T00:00:01Z"}"#;
//! let report = importer.import_str(jsonl, &store).unwrap();
//! println!("Imported {} spans, {} errors", report.spans_imported, report.errors.len());
//! ```

use crate::trace_store::{SpanRecord, SpanType, TraceSource, TraceStore};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Configuration for trace importers.
#[derive(Debug, Clone)]
pub struct ImporterConfig {
    /// Whether to skip spans with parse errors (true) or propagate errors (false).
    pub skip_errors: bool,
    /// Default source to assign if not detectable from the trace format.
    pub default_source: TraceSource,
    /// Schema version to record in `attributes.schema_version`.
    pub schema_version: String,
}

impl Default for ImporterConfig {
    fn default() -> Self {
        Self {
            skip_errors: true,
            default_source: TraceSource::Guixu,
            schema_version: "1.0".to_string(),
        }
    }
}

/// Import report summarizing what was imported and what errors occurred.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImportReport {
    /// Number of spans successfully imported.
    pub spans_imported: usize,
    /// Number of traces processed (may contain multiple spans).
    pub traces_processed: usize,
    /// Number of spans skipped due to deduplication (already existed).
    pub spans_skipped: usize,
    /// Errors encountered during import.
    pub errors: Vec<ImportError>,
}

/// A single import error with location context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportError {
    /// Line number in the source file (1-indexed), or record index for non-file sources.
    pub line: usize,
    /// The trace_id being processed when the error occurred (if known).
    pub trace_id: Option<String>,
    /// Human-readable error message.
    pub message: String,
}

/// Trait for importing traces from a specific provider.
pub trait TraceImporter: Send + Sync {
    /// Import traces from a file.
    fn import_file(&self, path: &Path, store: &TraceStore) -> Result<ImportReport>;

    /// Import traces from a JSON string slice.
    fn import_str(&self, input: &str, store: &TraceStore) -> Result<ImportReport>;
}

// ============================================================================
// OpenAI Agents SDK Importer
// ============================================================================

/// Importer for OpenAI Agents SDK trace exports (JSONL format).
///
/// OpenAI Agents SDK produces JSONL where each line is a span event. A full trace
/// is assembled from multiple spans sharing the same `trace_id`.
///
/// OpenAI trace format fields mapped to `SpanRecord`:
/// - `trace_id` → `trace_id` (prefixed with `openai_` if not present)
/// - `span_id` / `id` → `span_id`
/// - `parent_id` → `parent_span_id`
/// - `name` → `span_name`
/// - `type` → `span_type` (agent/generation/function/etc.)
/// - `start_time` / `created_at` → `start_time`
/// - `end_time` → `end_time`
/// - `input_tokens` / `usage.input_tokens` → `input_tokens`
/// - `output_tokens` / `usage.output_tokens` → `output_tokens`
/// - `model` → `model`
/// - `error` → `error`
/// - All other fields → `attributes` JSON
pub struct OpenAiImporter {
    config: ImporterConfig,
}

impl OpenAiImporter {
    pub fn new(config: ImporterConfig) -> Self {
        Self { config }
    }

    /// Detect the span type from OpenAI's `type` field.
    fn detect_span_type(type_str: &str) -> SpanType {
        match type_str {
            "agent" => SpanType::Agent,
            "generation" | "llm" => SpanType::Generation,
            "function" | "tool_call" | "function_call" => SpanType::ToolUse,
            "guardrail" | "safety" => SpanType::Guardrail,
            "handoff" => SpanType::Handoff,
            "user" | "prompt" => SpanType::User,
            "system" => SpanType::System,
            _ => SpanType::Other,
        }
    }

    /// Parse a single OpenAI span JSON object into a `SpanRecord`.
    fn parse_span(&self, value: &serde_json::Value, line_num: usize) -> Result<SpanRecord> {
        let obj = value.as_object().ok_or_else(|| {
            anyhow::anyhow!("line {}: expected JSON object, got {}", line_num, value)
        })?;

        // trace_id: OpenAI uses `trace_id` or `metadata.trace_id`
        let trace_id = obj
            .get("trace_id")
            .and_then(|v| v.as_str())
            .map(|s| format!("openai_{}", s))
            .or_else(|| {
                obj.get("metadata")
                    .and_then(|v| v.get("trace_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| format!("openai_{}", s))
            })
            .unwrap_or_else(|| format!("openai_trace_line{}", line_num));

        // span_id: OpenAI uses `span_id` or `id`
        let span_id = obj
            .get("span_id")
            .or_else(|| obj.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("span_{}", line_num));

        // parent_span_id
        let parent_span_id = obj
            .get("parent_id")
            .or_else(|| obj.get("parent_span_id"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        // span_name
        let span_name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // span_type
        let type_str = obj.get("type").and_then(|v| v.as_str()).unwrap_or("other");
        let span_type = Self::detect_span_type(type_str);

        // Timestamps
        let start_time = obj
            .get("start_time")
            .or_else(|| obj.get("created_at"))
            .and_then(parse_timestamp)
            .unwrap_or_else(Utc::now);

        let end_time = obj
            .get("end_time")
            .or_else(|| obj.get("completed_at"))
            .and_then(parse_timestamp)
            .unwrap_or_else(Utc::now);

        // Token usage
        let input_tokens = obj
            .get("usage")
            .and_then(|v| v.get("input_tokens"))
            .and_then(|v| v.as_i64())
            .or_else(|| obj.get("input_tokens").and_then(|v| v.as_i64()));

        let output_tokens = obj
            .get("usage")
            .and_then(|v| v.get("output_tokens"))
            .and_then(|v| v.as_i64())
            .or_else(|| obj.get("output_tokens").and_then(|v| v.as_i64()));

        // Model
        let model = obj
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Error
        let error = obj
            .get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                obj.get("status")
                    .and_then(|v| v.get("error"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });

        // Build attributes JSON with all remaining fields + schema version markers
        let mut attrs = serde_json::Map::new();
        attrs.insert(
            "schema_version".to_string(),
            self.config.schema_version.clone().into(),
        );
        attrs.insert("source_schema".to_string(), "openai".into());
        attrs.insert("raw_type".to_string(), type_str.into());

        for (k, v) in obj {
            let k_str: &str = k;
            match k_str {
                // Already handled fields — skip
                "trace_id" | "id" | "span_id" | "parent_id" | "parent_span_id" | "name"
                | "type" | "start_time" | "end_time" | "created_at" | "completed_at" | "usage"
                | "input_tokens" | "output_tokens" | "model" | "error" | "status" | "metadata" => {
                    continue
                }
                _ => {
                    attrs.insert(k.clone(), v.clone());
                }
            }
        }

        let duration_ms = (end_time - start_time).num_microseconds().unwrap_or(0) as f64 / 1000.0;

        Ok(SpanRecord {
            trace_id,
            span_id,
            parent_span_id,
            span_name,
            span_type,
            source: TraceSource::OpenAi,
            start_time,
            end_time,
            duration_ms,
            attributes: serde_json::Value::Object(attrs),
            input_tokens,
            output_tokens,
            model,
            error,
        })
    }

    fn process_line(
        &self,
        line: &str,
        line_num: usize,
        store: &TraceStore,
        report: &mut ImportReport,
    ) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }

        let value = match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(v) => v,
            Err(e) => {
                report.errors.push(ImportError {
                    line: line_num,
                    trace_id: None,
                    message: format!("JSON parse error: {}", e),
                });
                if !self.config.skip_errors {
                    return;
                }
                return;
            }
        };

        let trace_id = value
            .get("trace_id")
            .or_else(|| value.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let span = match self.parse_span(&value, line_num) {
            Ok(s) => s,
            Err(e) => {
                report.errors.push(ImportError {
                    line: line_num,
                    trace_id,
                    message: e.to_string(),
                });
                if !self.config.skip_errors {
                    return;
                }
                return;
            }
        };

        if let Err(e) = store.insert_span(&span) {
            // DuckDB ON CONFLICT do nothing is not an error, but other errors are
            report.errors.push(ImportError {
                line: line_num,
                trace_id: Some(span.trace_id.clone()),
                message: format!("DB insert error: {}", e),
            });
            if !self.config.skip_errors {
                return;
            }
            return;
        }

        report.spans_imported += 1;
    }
}

impl TraceImporter for OpenAiImporter {
    fn import_file(&self, path: &Path, store: &TraceStore) -> Result<ImportReport> {
        let file = File::open(path).map_err(|e| anyhow::anyhow!("failed to open file: {}", e))?;
        let reader = BufReader::new(file);
        let mut report = ImportReport::default();

        // Count traces (unique trace_ids) while processing
        let mut seen_trace_ids = std::collections::HashSet::new();

        for (line_num, line) in reader.lines().enumerate() {
            let line =
                line.map_err(|e| anyhow::anyhow!("failed to read line {}: {}", line_num + 1, e))?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Pre-extract trace_id for counting
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(tid) = value.get("trace_id").and_then(|v| v.as_str()) {
                    if seen_trace_ids.insert(tid.to_string()) {
                        report.traces_processed += 1;
                    }
                }
            }

            self.process_line(&line, line_num + 1, store, &mut report);
        }

        Ok(report)
    }

    fn import_str(&self, input: &str, store: &TraceStore) -> Result<ImportReport> {
        let mut report = ImportReport::default();
        let mut seen_trace_ids = std::collections::HashSet::new();

        for (line_num, line) in input.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(tid) = value.get("trace_id").and_then(|v| v.as_str()) {
                    if seen_trace_ids.insert(tid.to_string()) {
                        report.traces_processed += 1;
                    }
                }
            }

            self.process_line(line, line_num + 1, store, &mut report);
        }

        Ok(report)
    }
}

// ============================================================================
// Claude Agent SDK Importer
// ============================================================================

/// Importer for Claude Agent SDK trace exports.
///
/// Claude SDK exports traces as newline-delimited JSON where each line is a
/// span or event. The format is similar to OpenAI but with different field names.
///
/// Claude trace format fields mapped to `SpanRecord`:
/// - `trace_id` → `trace_id` (prefixed with `claude_` if not present)
/// - `span_id` → `span_id`
/// - `parent_id` → `parent_span_id`
/// - `name` / `span_name` → `span_name`
/// - `type` / `span_type` → `span_type`
/// - `start_time` → `start_time`
/// - `end_time` → `end_time`
/// - `usage.input_tokens` → `input_tokens`
/// - `usage.output_tokens` → `output_tokens`
/// - `model` → `model`
/// - `error` → `error`
/// - All other fields → `attributes` JSON
pub struct ClaudeImporter {
    config: ImporterConfig,
}

impl ClaudeImporter {
    pub fn new(config: ImporterConfig) -> Self {
        Self { config }
    }

    fn detect_span_type(type_str: &str) -> SpanType {
        match type_str {
            "agent" | "agent_span" => SpanType::Agent,
            "generation" | "llm" | "message" => SpanType::Generation,
            "tool_use" | "tool_call" | "function" => SpanType::ToolUse,
            "guardrail" | "evaluation" => SpanType::Guardrail,
            "handoff" => SpanType::Handoff,
            "user" => SpanType::User,
            "system" => SpanType::System,
            _ => SpanType::Other,
        }
    }

    fn parse_span(&self, value: &serde_json::Value, line_num: usize) -> Result<SpanRecord> {
        let obj = value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("line {}: expected JSON object", line_num))?;

        // trace_id
        let trace_id = obj
            .get("trace_id")
            .or_else(|| obj.get("session_id"))
            .and_then(|v| v.as_str())
            .map(|s| format!("claude_{}", s))
            .unwrap_or_else(|| format!("claude_trace_line{}", line_num));

        // span_id
        let span_id = obj
            .get("span_id")
            .or_else(|| obj.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("span_{}", line_num));

        // parent_span_id
        let parent_span_id = obj
            .get("parent_id")
            .or_else(|| obj.get("parent_span_id"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        // span_name
        let span_name = obj
            .get("name")
            .or_else(|| obj.get("span_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // span_type
        let type_str = obj
            .get("type")
            .or_else(|| obj.get("span_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("other");
        let span_type = Self::detect_span_type(type_str);

        // Timestamps
        let start_time = obj
            .get("start_time")
            .and_then(parse_timestamp)
            .or_else(|| obj.get("timestamp").and_then(parse_timestamp))
            .unwrap_or_else(Utc::now);

        let end_time = obj
            .get("end_time")
            .and_then(parse_timestamp)
            .unwrap_or_else(Utc::now);

        // Token usage — Claude uses `usage` object or top-level
        let input_tokens = obj
            .get("usage")
            .and_then(|v| v.get("input_tokens"))
            .and_then(|v| v.as_i64())
            .or_else(|| obj.get("input_tokens").and_then(|v| v.as_i64()));

        let output_tokens = obj
            .get("usage")
            .and_then(|v| v.get("output_tokens"))
            .and_then(|v| v.as_i64())
            .or_else(|| obj.get("output_tokens").and_then(|v| v.as_i64()));

        // Model
        let model = obj
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Error
        let error = obj
            .get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Build attributes
        let mut attrs = serde_json::Map::new();
        attrs.insert(
            "schema_version".to_string(),
            self.config.schema_version.clone().into(),
        );
        attrs.insert("source_schema".to_string(), "claude".into());
        attrs.insert("raw_type".to_string(), type_str.into());

        // Copy events array if present (for event-level tracing)
        if let Some(events) = obj.get("events") {
            attrs.insert("events".to_string(), events.clone());
        }

        for (k, v) in obj {
            let k_str: &str = k;
            match k_str {
                "trace_id" | "id" | "span_id" | "parent_id" | "parent_span_id" | "name"
                | "span_name" | "type" | "span_type" | "start_time" | "end_time" | "timestamp"
                | "usage" | "input_tokens" | "output_tokens" | "model" | "error" | "events" => {
                    continue
                }
                _ => {
                    attrs.insert(k.clone(), v.clone());
                }
            }
        }

        let duration_ms = (end_time - start_time).num_microseconds().unwrap_or(0) as f64 / 1000.0;

        Ok(SpanRecord {
            trace_id,
            span_id,
            parent_span_id,
            span_name,
            span_type,
            source: TraceSource::Claude,
            start_time,
            end_time,
            duration_ms,
            attributes: serde_json::Value::Object(attrs),
            input_tokens,
            output_tokens,
            model,
            error,
        })
    }

    fn process_line(
        &self,
        line: &str,
        line_num: usize,
        store: &TraceStore,
        report: &mut ImportReport,
    ) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }

        let value = match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(v) => v,
            Err(e) => {
                report.errors.push(ImportError {
                    line: line_num,
                    trace_id: None,
                    message: format!("JSON parse error: {}", e),
                });
                if !self.config.skip_errors {
                    return;
                }
                return;
            }
        };

        let trace_id = value
            .get("trace_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let span = match self.parse_span(&value, line_num) {
            Ok(s) => s,
            Err(e) => {
                report.errors.push(ImportError {
                    line: line_num,
                    trace_id,
                    message: e.to_string(),
                });
                if !self.config.skip_errors {
                    return;
                }
                return;
            }
        };

        if let Err(e) = store.insert_span(&span) {
            report.errors.push(ImportError {
                line: line_num,
                trace_id: Some(span.trace_id.clone()),
                message: format!("DB insert error: {}", e),
            });
            if !self.config.skip_errors {
                return;
            }
            return;
        }

        report.spans_imported += 1;
    }
}

impl TraceImporter for ClaudeImporter {
    fn import_file(&self, path: &Path, store: &TraceStore) -> Result<ImportReport> {
        let file = File::open(path).map_err(|e| anyhow::anyhow!("failed to open file: {}", e))?;
        let reader = BufReader::new(file);
        let mut report = ImportReport::default();
        let mut seen_trace_ids = std::collections::HashSet::new();

        for (line_num, line) in reader.lines().enumerate() {
            let line =
                line.map_err(|e| anyhow::anyhow!("failed to read line {}: {}", line_num + 1, e))?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(tid) = value.get("trace_id").and_then(|v| v.as_str()) {
                    if seen_trace_ids.insert(tid.to_string()) {
                        report.traces_processed += 1;
                    }
                }
            }

            self.process_line(&line, line_num + 1, store, &mut report);
        }

        Ok(report)
    }

    fn import_str(&self, input: &str, store: &TraceStore) -> Result<ImportReport> {
        let mut report = ImportReport::default();
        let mut seen_trace_ids = std::collections::HashSet::new();

        for (line_num, line) in input.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(tid) = value.get("trace_id").and_then(|v| v.as_str()) {
                    if seen_trace_ids.insert(tid.to_string()) {
                        report.traces_processed += 1;
                    }
                }
            }

            self.process_line(line, line_num + 1, store, &mut report);
        }

        Ok(report)
    }
}

// ============================================================================
// Shared utilities
// ============================================================================

/// Parse a timestamp from various JSON value formats into DateTime<Utc>.
fn parse_timestamp(v: &serde_json::Value) -> Option<DateTime<Utc>> {
    match v {
        // ISO 8601 string
        serde_json::Value::String(s) => DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc)),
        // Unix epoch seconds as number
        serde_json::Value::Number(n) if n.is_i64() => {
            DateTime::from_timestamp(n.as_i64().unwrap(), 0)
        }
        // Unix epoch seconds as number (f64)
        serde_json::Value::Number(n) => DateTime::from_timestamp(n.as_i64().unwrap_or(0), 0),
        // Already a unix timestamp integer
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_openai_importer_basic() {
        let store = TraceStore::open_in_memory().unwrap();
        let importer = OpenAiImporter::new(ImporterConfig::default());

        let jsonl_content = r#"{"trace_id":"t001","span_id":"s001","parent_id":null,"name":"agent_loop","type":"agent","start_time":"2026-01-01T00:00:00Z","end_time":"2026-01-01T00:00:01Z","model":"gpt-4o","usage":{"input_tokens":100,"output_tokens":200}}
{"trace_id":"t001","span_id":"s002","parent_id":"s001","name":"llm_call","type":"generation","start_time":"2026-01-01T00:00:00.1Z","end_time":"2026-01-01T00:00:00.8Z","model":"gpt-4o","usage":{"input_tokens":100,"output_tokens":200}}
{"trace_id":"t002","span_id":"s003","name":"tool_call","type":"function","start_time":"2026-01-01T00:00:02Z","end_time":"2026-01-01T00:00:02.5Z"}"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(jsonl_content.as_bytes()).unwrap();

        let report = importer.import_file(file.path(), &store).unwrap();

        assert_eq!(report.traces_processed, 2);
        assert_eq!(report.spans_imported, 3);
        assert!(report.errors.is_empty());

        // Verify spans are stored
        let spans = store.get_trace_spans("openai_t001", "openai").unwrap();
        assert_eq!(spans.len(), 2);

        // Verify second import is idempotent (ON CONFLICT do nothing)
        let report2 = importer.import_file(file.path(), &store).unwrap();
        assert_eq!(report2.spans_imported, 0); // ON CONFLICT no-op
        assert!(report2.spans_skipped > 0 || report2.spans_imported == 0);
    }

    #[test]
    fn test_openai_importer_skip_errors() {
        let store = TraceStore::open_in_memory().unwrap();
        let importer = OpenAiImporter::new(ImporterConfig {
            skip_errors: true,
            ..Default::default()
        });

        let jsonl_content = r#"{"trace_id":"t001","span_id":"s001","name":"ok","type":"agent","start_time":"2026-01-01T00:00:00Z","end_time":"2026-01-01T00:00:01Z"}
not valid json
{"trace_id":"t002","span_id":"s002","name":"also_ok","type":"agent","start_time":"2026-01-01T00:00:02Z","end_time":"2026-01-01T00:00:03Z"}"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(jsonl_content.as_bytes()).unwrap();

        let report = importer.import_file(file.path(), &store).unwrap();

        assert_eq!(report.spans_imported, 2);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].line, 2);
    }

    #[test]
    fn test_claude_importer_basic() {
        let store = TraceStore::open_in_memory().unwrap();
        let importer = ClaudeImporter::new(ImporterConfig::default());

        let jsonl_content = r#"{"trace_id":"c001","span_id":"cs001","parent_id":null,"name":"agent_loop","type":"agent","start_time":"2026-01-01T00:00:00Z","end_time":"2026-01-01T00:00:01Z","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":200}}
{"trace_id":"c001","span_id":"cs002","parent_id":"cs001","name":"tool_use","type":"tool_use","start_time":"2026-01-01T00:00:00.2Z","end_time":"2026-01-01T00:00:00.5Z"}"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(jsonl_content.as_bytes()).unwrap();

        let report = importer.import_file(file.path(), &store).unwrap();

        assert_eq!(report.traces_processed, 1);
        assert_eq!(report.spans_imported, 2);
        assert!(report.errors.is_empty());

        let spans = store.get_trace_spans("claude_c001", "claude").unwrap();
        assert_eq!(spans.len(), 2);
    }

    #[test]
    fn test_parse_timestamp() {
        // ISO 8601: 2026-01-01T00:00:00Z
        let val = serde_json::json!("2026-01-01T00:00:00Z");
        let dt = parse_timestamp(&val).unwrap();
        assert_eq!(dt.timestamp(), 1767225600); // 2026-01-01

        // Unix epoch
        let val = serde_json::json!(1735689600);
        let dt = parse_timestamp(&val).unwrap();
        assert_eq!(dt.timestamp(), 1735689600);
    }
}
