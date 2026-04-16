// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Active trace emission manager for guixu agent execution.
//!
//! Provides:
//! - In-memory span buffering with configurable flush triggers (batch size / time-based)
//! - Background flush task via `tokio::spawn`
//! - W3C TraceContext propagation
//! - Drop-based flush via `ShutdownGuard`
//!
//! The manager is `Send + Sync` and can be safely shared across async tasks.
//!
//! # Example
//!
//! ```
//! use data_storage::trace_manager::{AgentTraceManager, TraceConfig, SpanType};
//!
//! let config = TraceConfig::default();
//! let manager = AgentTraceManager::new(config);
//! ```

use crate::otel_export::{OtelExportConfig, OtelExportHandle};
pub use crate::trace_store::SpanType;
use crate::trace_store::{SpanRecord, TraceSource, TraceStore};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// TraceConfig
// ---------------------------------------------------------------------------

/// Configuration for the trace emission manager.
#[derive(Debug, Clone)]
pub struct TraceConfig {
    /// Enable active trace emission (default: false — opt-in).
    pub enabled: bool,
    /// Path to the DuckDB trace database.
    pub db_path: String,
    /// Flush when buffer reaches this size.
    pub buffer_size: usize,
    /// Flush interval in seconds.
    pub flush_interval_secs: u64,
    /// Sampling rate (0.0 to 1.0, default: 1.0 = 100%).
    pub sample_rate: f64,
    /// Auto-export path for JSONL traces (optional).
    pub auto_export_path: Option<String>,
    /// Enable OTLP export of spans using OTel GenAI semantic conventions.
    pub otel_enabled: bool,
    /// OTLP endpoint (e.g. `http://localhost:4318`).
    pub otel_endpoint: String,
    /// Service name for OTel resource attribute.
    pub otel_service_name: String,
    /// Optional auth header for OTLP endpoint.
    pub otel_auth_header: Option<String>,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            db_path: "traces.duckdb".into(),
            buffer_size: 100,
            flush_interval_secs: 30,
            sample_rate: 1.0,
            auto_export_path: None,
            otel_enabled: false,
            otel_endpoint: "http://localhost:4318".into(),
            otel_service_name: "guixu".into(),
            otel_auth_header: None,
        }
    }
}

// ---------------------------------------------------------------------------
// TraceContext (W3C TraceContext)
// ---------------------------------------------------------------------------

/// W3C TraceContext traceparent header.
/// Format: `00-{trace_id}-{span_id}-{flags}`
/// - version: "00" (1 byte)
/// - trace_id: 32 hex chars (16 bytes)
/// - span_id: 16 hex chars (8 bytes)
/// - flags: 2 hex chars (1 byte) — "01" = sampled
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    /// Trace ID (UUID v4, 128 bits).
    pub trace_id: Uuid,
    /// Span ID (64 bits).
    pub span_id: u64,
    /// Whether this span is sampled.
    pub sampled: bool,
}

impl TraceContext {
    /// Parse from a traceparent header string.
    ///
    /// Returns `None` if the string is malformed.
    pub fn from_traceparent(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 {
            return None;
        }
        if parts[0] != "00" {
            return None;
        }
        let trace_id = Uuid::parse_str(parts[1]).ok()?;
        let span_id = u64::from_str_radix(parts[2], 16).ok()?;
        let flags = u8::from_str_radix(parts[3], 16).ok()?;
        Some(Self {
            trace_id,
            span_id,
            sampled: (flags & 0x01) != 0,
        })
    }

    /// Serialize to a traceparent header string.
    pub fn to_traceparent(&self) -> String {
        format!(
            "00-{:032x}-{:016x}-{:02x}",
            self.trace_id.as_u128(),
            self.span_id,
            if self.sampled { 0x01 } else { 0x00 }
        )
    }

    /// Generate a new root `TraceContext` with a fresh `trace_id` and random `span_id`.
    pub fn new_root() -> Self {
        Self {
            trace_id: Uuid::new_v4(),
            span_id: rand_u64(),
            sampled: true,
        }
    }

    /// Create a child span context (same `trace_id`, new `span_id`).
    pub fn child_span_id(&self, span_id: u64) -> Self {
        Self {
            trace_id: self.trace_id,
            span_id,
            sampled: self.sampled,
        }
    }
}

/// Generate a random u64 using the standard library's random state.
fn rand_u64() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    RandomState::new().build_hasher().finish()
}

// ---------------------------------------------------------------------------
// SpanBuilder
// ---------------------------------------------------------------------------

/// Builder for creating spans with automatic fields.
/// Call `.finish()` to produce a [`SpanRecord`].
///
/// # Example
///
/// ```
/// use data_storage::trace_manager::SpanBuilder;
/// use data_storage::trace_store::SpanType;
///
/// let span = SpanBuilder::new(
///     "trace_001",
///     "span_001",
///     None,
///     "workflow.run",
///     SpanType::Agent,
/// )
/// .with_attribute("job_id", serde_json::json!("job_123"))
/// .with_input_tokens(100)
/// .with_output_tokens(250)
/// .finish();
/// ```
pub struct SpanBuilder {
    trace_id: String,
    span_id: String,
    parent_span_id: Option<String>,
    session_id: Option<String>,
    span_name: String,
    span_type: SpanType,
    start_time: DateTime<Utc>,
    attributes: serde_json::Map<String, serde_json::Value>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    model: Option<String>,
    error: Option<String>,
}

impl SpanBuilder {
    /// Create a new builder for a span.
    pub fn new(
        trace_id: &str,
        span_id: &str,
        parent_span_id: Option<&str>,
        span_name: &str,
        span_type: SpanType,
    ) -> Self {
        Self {
            trace_id: trace_id.to_string(),
            span_id: span_id.to_string(),
            parent_span_id: parent_span_id.map(String::from),
            session_id: None,
            span_name: span_name.to_string(),
            span_type,
            start_time: Utc::now(),
            attributes: serde_json::Map::new(),
            input_tokens: None,
            output_tokens: None,
            model: None,
            error: None,
        }
    }

    /// Add a JSON attribute.
    pub fn with_attribute(mut self, key: &str, value: serde_json::Value) -> Self {
        self.attributes.insert(key.to_string(), value);
        self
    }

    /// Set input token count.
    pub fn with_input_tokens(mut self, tokens: i64) -> Self {
        self.input_tokens = Some(tokens);
        self
    }

    /// Set output token count.
    pub fn with_output_tokens(mut self, tokens: i64) -> Self {
        self.output_tokens = Some(tokens);
        self
    }

    /// Set model name.
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = Some(model.to_string());
        self
    }

    /// Set error message.
    pub fn with_error(mut self, error: &str) -> Self {
        self.error = Some(error.to_string());
        self
    }

    /// Set session id.
    pub fn with_session_id(mut self, session_id: &str) -> Self {
        self.session_id = Some(session_id.to_string());
        self
    }

    /// Finish building and return the [`SpanRecord`].
    pub fn finish(self) -> SpanRecord {
        let end_time = Utc::now();
        let duration_ms =
            (end_time - self.start_time).num_microseconds().unwrap_or(0) as f64 / 1000.0;

        SpanRecord {
            trace_id: self.trace_id,
            span_id: self.span_id,
            parent_span_id: self.parent_span_id,
            session_id: self.session_id,
            span_name: self.span_name,
            span_type: self.span_type,
            source: TraceSource::Guixu,
            start_time: self.start_time,
            end_time,
            duration_ms,
            attributes: serde_json::Value::Object(self.attributes),
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            model: self.model,
            error: self.error,
        }
    }
}

// ---------------------------------------------------------------------------
// FlushTask
// ---------------------------------------------------------------------------

enum FlushCommand {
    Flush(Vec<SpanRecord>),
    Shutdown,
}

async fn flush_task(
    db_path: String,
    mut rx: mpsc::Receiver<FlushCommand>,
    flush_interval: Duration,
    auto_export_path: Option<String>,
    otel_handle: OtelExportHandle,
) {
    let mut interval = tokio::time::interval(flush_interval);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Time-based flush tick — the actual flush is triggered via channel
            }
            cmd = rx.recv() => {
                match cmd {
                    Some(FlushCommand::Flush(spans)) => {
                        if !spans.is_empty() {
                            // Export to OTel (async-safe, non-blocking)
                            otel_handle.export_spans(&spans).await;

                            let db_path = db_path.clone();
                            let auto_export = auto_export_path.clone();
                            let spans_for_export = spans.clone();
                            tokio::task::spawn_blocking(move || {
                                // Open a fresh connection for each flush to avoid Sync issues
                                let store = TraceStore::open(std::path::Path::new(&db_path));
                                if let Ok(store) = store {
                                    if let Err(e) = store.insert_spans(&spans) {
                                        tracing::error!(error = %e, "failed to flush spans to store");
                                    }
                                }
                                if let Some(path) = &auto_export {
                                    if let Err(e) = export_spans_to_jsonl(&spans_for_export, path) {
                                        tracing::warn!(error = %e, "auto-export failed");
                                    }
                                }
                            })
                            .await
                            .ok();
                        }
                    }
                    None | Some(FlushCommand::Shutdown) => {
                        otel_handle.shutdown().await;
                        break;
                    }
                }
            }
        }
    }
}

fn export_spans_to_jsonl(spans: &[SpanRecord], path: &str) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let mut writer = std::io::BufWriter::new(file);
    for span in spans {
        serde_json::to_writer(&mut writer, span)?;
        writeln!(&mut writer)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// AgentTraceManager
// ---------------------------------------------------------------------------

/// Central trace emission manager.
///
/// Thread-safe, buffered, supports W3C TraceContext propagation.
/// Designed to be stored in `Arc<RwLock<AgentTraceManager>>` so it can be
/// shared across async tasks and accessed without cloning the manager.
///
/// This struct is `Send + Sync` because:
/// - `config` is `Clone + Send`
/// - `current_context` is `RwLock<Option<TraceContext>>` which is `Send`
/// - `buffer` is `RwLock<Vec<SpanRecord>>` which is `Send`
/// - `tx` is `mpsc::Sender<FlushCommand>` which is `Send`
pub struct AgentTraceManager {
    config: TraceConfig,
    current_context: RwLock<Option<TraceContext>>,
    buffer: RwLock<Vec<SpanRecord>>,
    tx: mpsc::Sender<FlushCommand>,
    _shutdown_guard: ShutdownGuard,
}

impl Clone for AgentTraceManager {
    /// Clone creates a **new, independent** manager that shares only the flush
    /// channel with the original. The `current_context` and `buffer` are NOT
    /// shared — the clone starts with empty context and an empty buffer.
    ///
    /// If you need shared state across tasks, wrap in `Arc<RwLock<AgentTraceManager>>`
    /// instead of cloning.
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            current_context: RwLock::new(None),
            buffer: RwLock::new(Vec::new()),
            tx: self.tx.clone(),
            _shutdown_guard: ShutdownGuard {
                tx: self.tx.clone(),
            },
        }
    }
}

/// RAII guard that signals shutdown to the flush task when dropped.
pub struct ShutdownGuard {
    tx: mpsc::Sender<FlushCommand>,
}

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        if self.tx.try_send(FlushCommand::Shutdown).is_err() {
            // Channel full or closed — flush task may not receive shutdown signal.
            // This is best-effort; the flush task will exit when all senders drop.
            tracing::warn!("trace shutdown signal dropped: channel full or closed");
        }
    }
}

impl AgentTraceManager {
    /// Create a new manager with the given config.
    pub fn new(config: TraceConfig) -> Self {
        let (tx, rx) = mpsc::channel(config.buffer_size * 2);
        let tx_clone = tx.clone();

        let otel_handle = if config.otel_enabled {
            OtelExportHandle::new(&OtelExportConfig {
                endpoint: config.otel_endpoint.clone(),
                service_name: config.otel_service_name.clone(),
                auth_header: config.otel_auth_header.clone(),
                timeout_secs: 10,
            })
        } else {
            OtelExportHandle::disabled()
        };

        tokio::spawn(flush_task(
            config.db_path.clone(),
            rx,
            Duration::from_secs(config.flush_interval_secs),
            config.auto_export_path.clone(),
            otel_handle,
        ));

        Self {
            config,
            current_context: RwLock::new(None),
            buffer: RwLock::new(Vec::new()),
            tx,
            _shutdown_guard: ShutdownGuard { tx: tx_clone },
        }
    }

    /// Create a manager from `NodeConfig`.
    pub fn from_config(config: &data_core::config::NodeConfig) -> anyhow::Result<Self> {
        let trace_cfg = &config.trace;
        let trace_config = TraceConfig {
            enabled: trace_cfg.enabled,
            db_path: trace_cfg.db_path.clone(),
            buffer_size: trace_cfg.buffer_size,
            flush_interval_secs: trace_cfg.flush_interval_secs,
            sample_rate: trace_cfg.sample_rate,
            auto_export_path: trace_cfg.auto_export_path.clone(),
            otel_enabled: trace_cfg.otel_enabled,
            otel_endpoint: trace_cfg.otel_endpoint.clone(),
            otel_service_name: trace_cfg.otel_service_name.clone(),
            otel_auth_header: trace_cfg.otel_auth_header.clone(),
        };

        let db_path = std::path::Path::new(&trace_config.db_path);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        Ok(Self::new(trace_config))
    }

    /// Returns `true` if tracing is enabled and sampling decides to sample this call.
    fn should_sample(&self) -> bool {
        if !self.config.enabled {
            return false;
        }
        if self.config.sample_rate >= 1.0 {
            return true;
        }
        use rand::Rng;
        rand::thread_rng().gen::<f64>() < self.config.sample_rate
    }

    /// Check if tracing is globally enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Start a new trace (root span).
    ///
    /// Returns `(trace_id, span_id)` if started, or `None` if disabled/not sampled.
    pub async fn start_trace(
        &self,
        span_name: &str,
        span_type: SpanType,
    ) -> Option<(String, String)> {
        if !self.should_sample() {
            return None;
        }

        let ctx = TraceContext::new_root();
        let span_id = format!("{:016x}", ctx.span_id);
        let trace_id = ctx.trace_id.to_string();

        let span = SpanBuilder::new(&trace_id, &span_id, None, span_name, span_type).finish();

        *self.current_context.write().await = Some(ctx);
        self.buffer_write(span).await;

        Some((trace_id, span_id))
    }

    /// Start a child span under the current trace.
    ///
    /// Returns the new `span_id` if a trace context is active, or `None`.
    pub async fn start_span(
        &self,
        parent_span_id: Option<&str>,
        span_name: &str,
        span_type: SpanType,
    ) -> Option<String> {
        if !self.should_sample() {
            return None;
        }

        let parent_ctx = {
            let ctx_guard = self.current_context.read().await;
            ctx_guard.as_ref()?.clone()
        };

        let span_id = rand_u64();
        let span_id_str = format!("{:016x}", span_id);
        let child_ctx = parent_ctx.child_span_id(span_id);

        let span = SpanBuilder::new(
            &parent_ctx.trace_id.to_string(),
            &span_id_str,
            parent_span_id,
            span_name,
            span_type,
        )
        .finish();

        *self.current_context.write().await = Some(child_ctx);
        self.buffer_write(span).await;

        Some(span_id_str)
    }

    /// End the current span and finalize it.
    pub async fn end_span(&self, builder: SpanBuilder) {
        if !self.config.enabled {
            return;
        }
        let span = builder.finish();

        // Pop context back to the parent span
        {
            let mut ctx_guard = self.current_context.write().await;
            if let Some(parent) = &*ctx_guard {
                let root = parent.child_span_id(0);
                *ctx_guard = Some(root);
            }
        }

        self.buffer_write(span).await;
    }

    /// Get the current trace context (if any).
    pub async fn current_context(&self) -> Option<TraceContext> {
        self.current_context.read().await.clone()
    }

    /// Set the current trace context (used for propagating context from HTTP headers).
    pub async fn set_current_context(&self, ctx: TraceContext) {
        *self.current_context.write().await = Some(ctx);
    }

    /// Set the current trace context synchronously (for use in non-async contexts).
    ///
    /// Logs a warning if the lock cannot be acquired (e.g. under high contention).
    pub fn set_current_context_sync(&self, ctx: TraceContext) {
        if let Ok(mut guard) = self.current_context.try_write() {
            *guard = Some(ctx);
        } else {
            tracing::warn!("failed to set trace context synchronously: lock contention");
        }
    }

    /// Force a flush of the current buffer to DuckDB.
    pub async fn flush(&self) {
        let spans = {
            let mut buffer = self.buffer.write().await;
            std::mem::take(&mut *buffer)
        };
        if !spans.is_empty() {
            let _ = self.tx.send(FlushCommand::Flush(spans)).await;
        }
    }

    async fn buffer_write(&self, span: SpanRecord) {
        let flush_spans = {
            let mut buffer = self.buffer.write().await;
            buffer.push(span);
            if buffer.len() >= self.config.buffer_size {
                Some(std::mem::take(&mut *buffer))
            } else {
                None
            }
        };
        if let Some(spans) = flush_spans {
            let _ = self.tx.try_send(FlushCommand::Flush(spans));
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_trace_context_roundtrip() {
        let ctx = TraceContext::new_root();
        let header = ctx.to_traceparent();
        let parsed = TraceContext::from_traceparent(&header).unwrap();
        assert_eq!(parsed.trace_id, ctx.trace_id);
        assert_eq!(parsed.span_id, ctx.span_id);
        assert_eq!(parsed.sampled, ctx.sampled);
    }

    #[tokio::test]
    async fn test_span_builder() {
        let span = SpanBuilder::new(
            "trace_001",
            "span_001",
            None,
            "workflow.run",
            SpanType::Agent,
        )
        .with_attribute("job_id", serde_json::json!("job_123"))
        .with_input_tokens(100)
        .with_output_tokens(250)
        .with_model("gpt-4o")
        .finish();

        assert_eq!(span.trace_id, "trace_001");
        assert_eq!(span.span_id, "span_001");
        assert_eq!(span.span_name, "workflow.run");
        assert_eq!(span.span_type, SpanType::Agent);
        assert_eq!(span.input_tokens, Some(100));
        assert_eq!(span.output_tokens, Some(250));
        assert_eq!(span.model, Some("gpt-4o".to_string()));
    }

    #[tokio::test]
    async fn test_manager_disabled() {
        let config = TraceConfig {
            enabled: false,
            ..Default::default()
        };
        let manager = AgentTraceManager::new(config);

        assert!(!manager.is_enabled());
        assert!(manager.start_trace("test", SpanType::Agent).await.is_none());
    }

    #[tokio::test]
    async fn test_manager_start_trace() {
        let config = TraceConfig::default();
        let manager = AgentTraceManager::new(config);

        let result = manager.start_trace("workflow.run", SpanType::Agent).await;
        assert!(result.is_some());

        let (trace_id, span_id) = result.unwrap();
        assert!(!trace_id.is_empty());
        assert!(!span_id.is_empty());

        manager.flush().await;
    }
}
