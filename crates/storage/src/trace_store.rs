// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! AI Agent Trace storage using DuckDB.
//!
//! Provides trace data management for guixu agents with support for:
//! - Storing and querying agent execution traces (spans)
//! - Importing external traces (OpenAI / Claude format)
//! - Privacy sanitization before sharing
//!
//! # Example
//!
//! ```
//! use data_storage::trace_store::{TraceStore, SpanRecord, SpanType};
//!
//! let store = TraceStore::open_in_memory().unwrap();
//!
//! let span: SpanRecord = SpanRecord::new(
//!     "trace_001",
//!     "span_001",
//!     None::<String>,
//!     "agent_loop",
//!     SpanType::Agent,
//! );
//! store.insert_span(&span).unwrap();
//!
//! let traces = store.list_traces("guixu", 10).unwrap();
//! ```

use anyhow::Result;
use chrono::{DateTime, TimeZone, Utc};
use duckdb::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// Span type classification following OpenTelemetry semantic conventions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpanType {
    /// Agent orchestration / main loop
    Agent,
    /// LLM generation (chat/completion)
    Generation,
    /// Tool / function call
    ToolUse,
    /// Guardrail / safety evaluation
    Guardrail,
    /// Handoff between agents
    Handoff,
    /// User interaction (prompt input)
    User,
    /// System event
    System,
    /// Unknown / other
    Other,
}

impl SpanType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpanType::Agent => "agent",
            SpanType::Generation => "generation",
            SpanType::ToolUse => "tool_use",
            SpanType::Guardrail => "guardrail",
            SpanType::Handoff => "handoff",
            SpanType::User => "user",
            SpanType::System => "system",
            SpanType::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "agent" => SpanType::Agent,
            "generation" => SpanType::Generation,
            "tool_use" => SpanType::ToolUse,
            "guardrail" => SpanType::Guardrail,
            "handoff" => SpanType::Handoff,
            "user" => SpanType::User,
            "system" => SpanType::System,
            _ => SpanType::Other,
        }
    }
}

/// Trace source identifier (guixu, openai, claude)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TraceSource {
    Guixu,
    OpenAi,
    Claude,
}

impl TraceSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            TraceSource::Guixu => "guixu",
            TraceSource::OpenAi => "openai",
            TraceSource::Claude => "claude",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "guixu" => TraceSource::Guixu,
            "openai" => TraceSource::OpenAi,
            "claude" => TraceSource::Claude,
            _ => TraceSource::Guixu,
        }
    }
}

/// A single span record in a trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanRecord {
    /// Unique trace identifier (e.g., trace_abc123)
    pub trace_id: String,
    /// Unique span identifier
    pub span_id: String,
    /// Parent span id for hierarchy (None for root span)
    pub parent_span_id: Option<String>,
    /// Human-readable span name
    pub span_name: String,
    /// Span type classification
    pub span_type: SpanType,
    /// Trace source (guixu, openai, claude)
    pub source: TraceSource,
    /// Span start time
    pub start_time: DateTime<Utc>,
    /// Span end time
    pub end_time: DateTime<Utc>,
    /// Duration in milliseconds
    pub duration_ms: f64,
    /// Arbitrary key-value attributes (JSON)
    pub attributes: serde_json::Value,
    /// Input token count (for LLM spans)
    pub input_tokens: Option<i64>,
    /// Output token count (for LLM spans)
    pub output_tokens: Option<i64>,
    /// Model name (for LLM spans)
    pub model: Option<String>,
    /// Error message if span failed
    pub error: Option<String>,
}

impl SpanRecord {
    /// Create a new span record
    pub fn new(
        trace_id: impl Into<String>,
        span_id: impl Into<String>,
        parent_span_id: Option<impl Into<String>>,
        span_name: impl Into<String>,
        span_type: SpanType,
    ) -> Self {
        let now = Utc::now();
        Self {
            trace_id: trace_id.into(),
            span_id: span_id.into(),
            parent_span_id: parent_span_id.map(Into::into),
            span_name: span_name.into(),
            span_type,
            source: TraceSource::Guixu,
            start_time: now,
            end_time: now,
            duration_ms: 0.0,
            attributes: serde_json::json!({}),
            input_tokens: None,
            output_tokens: None,
            model: None,
            error: None,
        }
    }

    /// Set start time
    pub fn with_start_time(mut self, t: DateTime<Utc>) -> Self {
        self.start_time = t;
        self
    }

    /// Set end time and calculate duration
    pub fn with_end_time(mut self, t: DateTime<Utc>) -> Self {
        self.end_time = t;
        self.duration_ms = (self.end_time - self.start_time)
            .num_microseconds()
            .unwrap_or(0) as f64
            / 1000.0;
        self
    }

    /// Set input tokens
    pub fn with_input_tokens(mut self, tokens: i64) -> Self {
        self.input_tokens = Some(tokens);
        self
    }

    /// Set output tokens
    pub fn with_output_tokens(mut self, tokens: i64) -> Self {
        self.output_tokens = Some(tokens);
        self
    }

    /// Set model name
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set error
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Set source
    pub fn with_source(mut self, source: TraceSource) -> Self {
        self.source = source;
        self
    }

    /// Add an attribute
    pub fn with_attribute(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        if let serde_json::Value::Object(ref mut map) = self.attributes {
            map.insert(key.into(), value);
        }
        self
    }
}

/// A trace summary (for listing traces)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSummary {
    pub trace_id: String,
    pub trace_name: Option<String>,
    pub source: TraceSource,
    pub first_span_time: DateTime<Utc>,
    pub last_span_time: DateTime<Utc>,
    pub total_duration_ms: f64,
    pub span_count: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
}

/// DuckDB-backed trace store for AI agent traces
#[derive(Clone)]
pub struct TraceStore {
    #[allow(clippy::arc_with_non_send_sync)]
    conn: Arc<Connection>,
}

/// Convert DateTime<Utc> to epoch microseconds for DuckDB storage
fn datetime_to_micros(dt: DateTime<Utc>) -> i64 {
    dt.timestamp_micros()
}

/// Convert epoch microseconds back to DateTime<Utc>
fn micros_to_datetime(ms: i64) -> DateTime<Utc> {
    match Utc.timestamp_micros(ms) {
        chrono::LocalResult::Single(dt) => dt,
        _ => Utc::now(),
    }
}

impl TraceStore {
    /// Open or create a trace database at the given path
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        #[allow(clippy::arc_with_non_send_sync)]
        let store = Self {
            conn: Arc::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Open an in-memory database (for testing or temporary use)
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        #[allow(clippy::arc_with_non_send_sync)]
        let store = Self {
            conn: Arc::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Initialize the DuckDB schema
    ///
    /// Schema design principles (per Phase 1):
    /// - Stable columns for core fields (trace_id, span_id, parent_span_id, span_type,
    ///   start_time, end_time, duration_ms, source, model, token counts, error)
    /// - Variable fields go into JSON: provider-specific and OTel GenAI semantic convention fields
    ///   go into `attributes` JSON to tolerate spec churn
    /// - Idempotency: (source, trace_id, span_id) is the unique key for deduplication
    /// - All time columns stored as BIGINT (microseconds since epoch) to avoid
    ///   TIMESTAMP casting issues with DuckDB; conversion happens at the Rust boundary
    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS traces (
                trace_id              VARCHAR NOT NULL,
                trace_name            VARCHAR,
                source                VARCHAR NOT NULL DEFAULT 'guixu',
                first_span_time_us    BIGINT NOT NULL,  -- microseconds since epoch
                last_span_time_us     BIGINT NOT NULL,
                total_duration_ms     DOUBLE PRECISION DEFAULT 0,
                span_count            BIGINT DEFAULT 0,
                total_input_tokens    BIGINT DEFAULT 0,
                total_output_tokens   BIGINT DEFAULT 0,
                metadata              JSON,
                PRIMARY KEY (trace_id, source)
            );

            CREATE TABLE IF NOT EXISTS trace_spans (
                trace_id          VARCHAR NOT NULL,
                span_id           VARCHAR NOT NULL,
                parent_span_id    VARCHAR,
                span_name         VARCHAR NOT NULL,
                span_type         VARCHAR NOT NULL DEFAULT 'other',
                source            VARCHAR NOT NULL DEFAULT 'guixu',
                start_time_us     BIGINT NOT NULL,   -- microseconds since epoch
                end_time_us       BIGINT NOT NULL,
                duration_ms       DOUBLE PRECISION NOT NULL DEFAULT 0,
                attributes        JSON,             -- all provider-specific / genai-dev fields
                input_tokens      BIGINT,
                output_tokens     BIGINT,
                model             VARCHAR,
                error             VARCHAR,
                PRIMARY KEY (source, trace_id, span_id)
            );

            -- Indexes for common query patterns
            CREATE INDEX IF NOT EXISTS idx_spans_trace_id
                ON trace_spans(trace_id, source);
            CREATE INDEX IF NOT EXISTS idx_spans_start_time
                ON trace_spans(source, start_time_us DESC);
            CREATE INDEX IF NOT EXISTS idx_spans_span_type
                ON trace_spans(span_type);
            "#,
        )?;
        Ok(())
    }

    /// Insert a single span
    pub fn insert_span(&self, span: &SpanRecord) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO trace_spans (
                trace_id, span_id, parent_span_id, span_name, span_type,
                source, start_time_us, end_time_us, duration_ms, attributes,
                input_tokens, output_tokens, model, error
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                span.trace_id,
                span.span_id,
                span.parent_span_id,
                span.span_name,
                span.span_type.as_str(),
                span.source.as_str(),
                datetime_to_micros(span.start_time),
                datetime_to_micros(span.end_time),
                span.duration_ms,
                span.attributes.to_string(),
                span.input_tokens,
                span.output_tokens,
                span.model,
                span.error,
            ],
        )?;

        // Upsert trace summary
        self.upsert_trace_summary(span)?;
        Ok(())
    }

    /// Upsert trace summary after inserting a span
    fn upsert_trace_summary(&self, span: &SpanRecord) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO traces (trace_id, trace_name, source, first_span_time_us, last_span_time_us,
                               total_duration_ms, span_count, total_input_tokens, total_output_tokens)
            VALUES (?, NULL, ?, ?, ?, ?, 1, ?, ?)
            ON CONFLICT (trace_id, source) DO UPDATE SET
                first_span_time_us  = CASE WHEN traces.first_span_time_us > EXCLUDED.first_span_time_us
                                         THEN traces.first_span_time_us ELSE EXCLUDED.first_span_time_us END,
                last_span_time_us   = CASE WHEN traces.last_span_time_us < EXCLUDED.last_span_time_us
                                         THEN traces.last_span_time_us ELSE EXCLUDED.last_span_time_us END,
                total_duration_ms = traces.total_duration_ms + EXCLUDED.total_duration_ms,
                span_count = traces.span_count + 1,
                total_input_tokens  = traces.total_input_tokens  + COALESCE(EXCLUDED.total_input_tokens, 0),
                total_output_tokens = traces.total_output_tokens + COALESCE(EXCLUDED.total_output_tokens, 0)
            "#,
            params![
                span.trace_id,
                span.source.as_str(),
                datetime_to_micros(span.start_time),
                datetime_to_micros(span.end_time),
                span.duration_ms,
                span.input_tokens.unwrap_or(0),
                span.output_tokens.unwrap_or(0),
            ],
        )?;
        Ok(())
    }

    /// Insert multiple spans in a batch.
    pub fn insert_spans(&self, spans: &[SpanRecord]) -> Result<()> {
        for span in spans {
            self.insert_span(span)?;
        }
        Ok(())
    }

    /// List trace summaries, most recent first
    pub fn list_traces(&self, source: &str, limit: i64) -> Result<Vec<TraceSummary>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT trace_id, trace_name, source, first_span_time_us, last_span_time_us,
                   total_duration_ms, span_count, total_input_tokens, total_output_tokens
            FROM traces
            WHERE source = ?
            ORDER BY last_span_time_us DESC
            LIMIT ?
            "#,
        )?;

        let rows = stmt.query_map(params![source, limit], |row| {
            Ok(TraceSummary {
                trace_id: row.get(0)?,
                trace_name: row.get(1)?,
                source: TraceSource::parse(&row.get::<_, String>(2)?),
                first_span_time: micros_to_datetime(row.get::<_, i64>(3)?),
                last_span_time: micros_to_datetime(row.get::<_, i64>(4)?),
                total_duration_ms: row.get(5)?,
                span_count: row.get(6)?,
                total_input_tokens: row.get(7)?,
                total_output_tokens: row.get(8)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get all spans for a given trace
    pub fn get_trace_spans(&self, trace_id: &str, source: &str) -> Result<Vec<SpanRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT trace_id, span_id, parent_span_id, span_name, span_type,
                   source, start_time_us, end_time_us, duration_ms, attributes,
                   input_tokens, output_tokens, model, error
            FROM trace_spans
            WHERE trace_id = ? AND source = ?
            ORDER BY start_time_us ASC
            "#,
        )?;

        let rows = stmt.query_map(params![trace_id, source], |row| {
            let attrs_str: String = row.get(9)?;
            let attributes: serde_json::Value =
                serde_json::from_str(&attrs_str).unwrap_or(serde_json::json!({}));
            Ok(SpanRecord {
                trace_id: row.get(0)?,
                span_id: row.get(1)?,
                parent_span_id: row.get(2)?,
                span_name: row.get(3)?,
                span_type: SpanType::parse(&row.get::<_, String>(4)?),
                source: TraceSource::parse(&row.get::<_, String>(5)?),
                start_time: micros_to_datetime(row.get::<_, i64>(6)?),
                end_time: micros_to_datetime(row.get::<_, i64>(7)?),
                duration_ms: row.get(8)?,
                attributes,
                input_tokens: row.get(10)?,
                output_tokens: row.get(11)?,
                model: row.get(12)?,
                error: row.get(13)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Delete a trace and all its spans
    pub fn delete_trace(&self, trace_id: &str, source: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM trace_spans WHERE trace_id = ? AND source = ?",
            params![trace_id, source],
        )?;
        self.conn.execute(
            "DELETE FROM traces WHERE trace_id = ? AND source = ?",
            params![trace_id, source],
        )?;
        Ok(())
    }

    /// Query spans with filters
    pub fn query_spans(
        &self,
        source: Option<&str>,
        span_type: Option<&str>,
        from_time: Option<DateTime<Utc>>,
        to_time: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<SpanRecord>> {
        let mut sql = String::from(
            r#"
            SELECT trace_id, span_id, parent_span_id, span_name, span_type,
                   source, start_time_us, end_time_us, duration_ms, attributes,
                   input_tokens, output_tokens, model, error
            FROM trace_spans WHERE 1=1
            "#,
        );
        let mut params_vec: Vec<Box<dyn duckdb::ToSql>> = Vec::new();

        if let Some(s) = source {
            sql.push_str(" AND source = ?");
            params_vec.push(Box::new(s.to_string()));
        }
        if let Some(t) = span_type {
            sql.push_str(" AND span_type = ?");
            params_vec.push(Box::new(t.to_string()));
        }
        if let Some(from) = from_time {
            sql.push_str(" AND start_time_us >= ?");
            params_vec.push(Box::new(datetime_to_micros(from)));
        }
        if let Some(to) = to_time {
            sql.push_str(" AND start_time_us <= ?");
            params_vec.push(Box::new(datetime_to_micros(to)));
        }
        sql.push_str(" ORDER BY start_time_us DESC LIMIT ?");
        params_vec.push(Box::new(limit));

        let params_refs: Vec<&dyn duckdb::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let attrs_str: String = row.get(9)?;
            let attributes: serde_json::Value =
                serde_json::from_str(&attrs_str).unwrap_or(serde_json::json!({}));
            Ok(SpanRecord {
                trace_id: row.get(0)?,
                span_id: row.get(1)?,
                parent_span_id: row.get(2)?,
                span_name: row.get(3)?,
                span_type: SpanType::parse(&row.get::<_, String>(4)?),
                source: TraceSource::parse(&row.get::<_, String>(5)?),
                start_time: micros_to_datetime(row.get::<_, i64>(6)?),
                end_time: micros_to_datetime(row.get::<_, i64>(7)?),
                duration_ms: row.get(8)?,
                attributes,
                input_tokens: row.get(10)?,
                output_tokens: row.get(11)?,
                model: row.get(12)?,
                error: row.get(13)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get a single span by id
    pub fn get_span(&self, span_id: &str) -> Result<Option<SpanRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT trace_id, span_id, parent_span_id, span_name, span_type,
                   source, start_time_us, end_time_us, duration_ms, attributes,
                   input_tokens, output_tokens, model, error
            FROM trace_spans WHERE span_id = ?
            "#,
        )?;

        let mut rows = stmt.query(params![span_id])?;
        if let Some(row) = rows.next()? {
            let attrs_str: String = row.get(9)?;
            let attributes: serde_json::Value =
                serde_json::from_str(&attrs_str).unwrap_or(serde_json::json!({}));
            Ok(Some(SpanRecord {
                trace_id: row.get(0)?,
                span_id: row.get(1)?,
                parent_span_id: row.get(2)?,
                span_name: row.get(3)?,
                span_type: SpanType::parse(&row.get::<_, String>(4)?),
                source: TraceSource::parse(&row.get::<_, String>(5)?),
                start_time: micros_to_datetime(row.get::<_, i64>(6)?),
                end_time: micros_to_datetime(row.get::<_, i64>(7)?),
                duration_ms: row.get(8)?,
                attributes,
                input_tokens: row.get(10)?,
                output_tokens: row.get(11)?,
                model: row.get(12)?,
                error: row.get(13)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Count total spans
    pub fn count_spans(&self, source: Option<&str>) -> Result<i64> {
        let sql = if source.is_some() {
            "SELECT COUNT(*) FROM trace_spans WHERE source = ?"
        } else {
            "SELECT COUNT(*) FROM trace_spans"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let count: i64 = if let Some(s) = source {
            stmt.query_row(params![s], |r| r.get(0))?
        } else {
            stmt.query_row([], |r| r.get(0))?
        };
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_trace_store_crud() {
        let store = TraceStore::open_in_memory().unwrap();

        let trace_id = "trace_test_001";
        let start = Utc::now();

        // Insert spans
        let span1 = SpanRecord::new(
            trace_id,
            "span_001",
            None::<String>,
            "agent_loop",
            SpanType::Agent,
        )
        .with_start_time(start)
        .with_end_time(start + Duration::milliseconds(100))
        .with_source(TraceSource::Guixu)
        .with_attribute("loop_count", serde_json::json!(1));

        let span2 = SpanRecord::new(
            trace_id,
            "span_002",
            Some("span_001"),
            "llm_generation",
            SpanType::Generation,
        )
        .with_start_time(start + Duration::milliseconds(10))
        .with_end_time(start + Duration::milliseconds(80))
        .with_source(TraceSource::Guixu)
        .with_input_tokens(100)
        .with_output_tokens(250)
        .with_model("gpt-4o");

        store.insert_spans(&[span1, span2]).unwrap();

        // Query spans
        let spans = store.get_trace_spans(trace_id, "guixu").unwrap();
        assert_eq!(spans.len(), 2);

        // List traces
        let traces = store.list_traces("guixu", 10).unwrap();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].trace_id, trace_id);
        assert_eq!(traces[0].span_count, 2);
        assert_eq!(traces[0].total_input_tokens, 100);
        assert_eq!(traces[0].total_output_tokens, 250);

        // Get single span
        let span = store.get_span("span_001").unwrap().unwrap();
        assert_eq!(span.span_name, "agent_loop");
        assert_eq!(span.span_type, SpanType::Agent);

        // Delete trace
        store.delete_trace(trace_id, "guixu").unwrap();
        let spans = store.get_trace_spans(trace_id, "guixu").unwrap();
        assert!(spans.is_empty());
    }

    #[test]
    fn test_query_spans() {
        let store = TraceStore::open_in_memory().unwrap();
        let now = Utc::now();

        for i in 0..5 {
            let trace_id = format!("trace_{}", i);
            let span = SpanRecord::new(
                &trace_id,
                format!("span_{}", i),
                None::<String>,
                if i % 2 == 0 { "agent_loop" } else { "llm_call" },
                if i % 2 == 0 {
                    SpanType::Agent
                } else {
                    SpanType::Generation
                },
            )
            .with_start_time(now - Duration::minutes(i as i64))
            .with_end_time(now - Duration::minutes(i as i64) + Duration::seconds(1))
            .with_source(TraceSource::Guixu);

            store.insert_span(&span).unwrap();
        }

        // Filter by span type
        let agent_spans = store
            .query_spans(Some("guixu"), Some("agent"), None, None, 100)
            .unwrap();
        assert_eq!(agent_spans.len(), 3); // 0, 2, 4

        // Filter by time range (traces 0, 1, 2, 3 started within last 3 minutes)
        let recent = store
            .query_spans(None, None, Some(now - Duration::minutes(3)), None, 100)
            .unwrap();
        assert_eq!(recent.len(), 4); // traces 0, 1, 2, 3
    }

    #[test]
    fn test_trace_summary() {
        let store = TraceStore::open_in_memory().unwrap();
        let now = Utc::now();

        let trace_id = "trace_summary_test";
        let spans = vec![
            SpanRecord::new(trace_id, "s1", None::<String>, "root", SpanType::Agent)
                .with_start_time(now)
                .with_end_time(now + Duration::seconds(1))
                .with_input_tokens(10)
                .with_output_tokens(20),
            SpanRecord::new(trace_id, "s2", Some("s1"), "child1", SpanType::Generation)
                .with_start_time(now + Duration::milliseconds(100))
                .with_end_time(now + Duration::milliseconds(900))
                .with_input_tokens(10)
                .with_output_tokens(20),
        ];

        store.insert_spans(&spans).unwrap();

        let summaries = store.list_traces("guixu", 10).unwrap();
        assert_eq!(summaries.len(), 1);
        let s = &summaries[0];
        assert_eq!(s.total_duration_ms, 1000.0 + 800.0); // ~1800ms total
        assert_eq!(s.span_count, 2);
        assert_eq!(s.total_input_tokens, 20);
        assert_eq!(s.total_output_tokens, 40);
    }
}
