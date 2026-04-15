// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! OTLP exporter that bridges Guixu `SpanRecord` to OpenTelemetry spans.
//!
//! Converts internal spans into OTel GenAI semantic convention spans and
//! exports them via OTLP (HTTP/protobuf) to any compatible collector.

use anyhow::Result;
use opentelemetry::trace::{SpanKind, Status, TraceId, Tracer, TracerProvider as _};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_otlp::WithHttpConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::otel_genai;
use crate::trace_store::SpanRecord;

// ---------------------------------------------------------------------------
// OtelExportConfig
// ---------------------------------------------------------------------------

/// Configuration for the OTLP exporter.
#[derive(Debug, Clone)]
pub struct OtelExportConfig {
    /// OTLP endpoint (e.g. `http://localhost:4318`).
    pub endpoint: String,
    /// Service name reported as `service.name` resource attribute.
    pub service_name: String,
    /// Optional bearer token or API key for the `Authorization` header.
    pub auth_header: Option<String>,
    /// Export timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for OtelExportConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4318".into(),
            service_name: "guixu".into(),
            auth_header: None,
            timeout_secs: 10,
        }
    }
}

// ---------------------------------------------------------------------------
// OtelExporter
// ---------------------------------------------------------------------------

/// Bridges Guixu `SpanRecord` to OTel spans and exports via OTLP.
pub struct OtelExporter {
    provider: Arc<SdkTracerProvider>,
}

impl OtelExporter {
    /// Initialize the OTLP exporter pipeline.
    pub fn new(config: &OtelExportConfig) -> Result<Self> {
        use opentelemetry_otlp::SpanExporter;
        use opentelemetry_sdk::trace::BatchSpanProcessor;
        use opentelemetry_sdk::Resource;

        let mut builder = SpanExporter::builder()
            .with_http()
            .with_endpoint(&config.endpoint)
            .with_timeout(std::time::Duration::from_secs(config.timeout_secs));

        if let Some(auth) = &config.auth_header {
            let mut headers = std::collections::HashMap::new();
            headers.insert("Authorization".to_string(), auth.clone());
            builder = builder.with_headers(headers);
        }

        let exporter = builder.build()?;

        let resource = Resource::builder()
            .with_service_name(config.service_name.clone())
            .build();

        let processor = BatchSpanProcessor::builder(exporter).build();

        let provider = SdkTracerProvider::builder()
            .with_span_processor(processor)
            .with_resource(resource)
            .build();

        Ok(Self {
            provider: Arc::new(provider),
        })
    }

    /// Export a batch of `SpanRecord` as OTel spans.
    pub fn export_spans(&self, spans: &[SpanRecord]) {
        let tracer = self.provider.tracer("guixu-genai");

        for span in spans {
            let otel_span_name = otel_genai::span_name(span);
            let attrs = otel_genai::span_attributes(span);

            let kind = match span.span_type {
                crate::trace_store::SpanType::Agent
                | crate::trace_store::SpanType::Generation
                | crate::trace_store::SpanType::Handoff => SpanKind::Client,
                crate::trace_store::SpanType::ToolUse | crate::trace_store::SpanType::Guardrail => {
                    SpanKind::Internal
                }
                _ => SpanKind::Internal,
            };

            // Build and immediately end the span with recorded timestamps.
            // OTel SDK doesn't expose direct timestamp injection on the public
            // Tracer API, so we create a span and end it — the attributes carry
            // the authoritative timing from the SpanRecord.
            let mut otel_span = tracer
                .span_builder(otel_span_name)
                .with_kind(kind)
                .with_attributes(attrs)
                .start(&tracer);

            if span.error.is_some() {
                opentelemetry::trace::Span::set_status(
                    &mut otel_span,
                    Status::error(span.error.clone().unwrap_or_else(|| "unknown".to_string())),
                );
            }

            opentelemetry::trace::Span::end(&mut otel_span);
        }
    }

    /// Flush pending spans and shut down the exporter.
    pub fn shutdown(&self) {
        if let Err(e) = self.provider.shutdown() {
            tracing::warn!(error = %e, "OTel exporter shutdown error");
        }
    }
}

// ---------------------------------------------------------------------------
// OtelExportHandle — async-safe wrapper
// ---------------------------------------------------------------------------

/// Async-safe handle for the OTel exporter, suitable for use in `TraceManager`.
///
/// Wraps `OtelExporter` behind `Arc<RwLock<>>` so it can be shared across
/// async tasks without `Sync` issues.
#[derive(Clone)]
pub struct OtelExportHandle {
    inner: Arc<RwLock<Option<OtelExporter>>>,
}

impl OtelExportHandle {
    /// Create a new handle from config. Returns a disabled handle on init failure.
    pub fn new(config: &OtelExportConfig) -> Self {
        let exporter = OtelExporter::new(config)
            .map_err(|e| {
                tracing::error!(error = %e, "failed to initialize OTel exporter");
                e
            })
            .ok();
        Self {
            inner: Arc::new(RwLock::new(exporter)),
        }
    }

    /// Create a disabled (no-op) handle.
    pub fn disabled() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    /// Export spans if the exporter is active.
    pub async fn export_spans(&self, spans: &[SpanRecord]) {
        let guard = self.inner.read().await;
        if let Some(exporter) = guard.as_ref() {
            exporter.export_spans(spans);
        }
    }

    /// Shut down the exporter.
    pub async fn shutdown(&self) {
        let guard = self.inner.read().await;
        if let Some(exporter) = guard.as_ref() {
            exporter.shutdown();
        }
    }
}

/// Parse a trace_id string (UUID or hex) into an OTel `TraceId`.
///
/// Accepts both UUID format (`550e8400-e29b-41d4-a716-446655440000`) and
/// raw 32-char hex. Returns `TraceId::INVALID` on parse failure.
pub fn parse_trace_id(s: &str) -> TraceId {
    // Strip hyphens for UUID format
    let hex: String = s.chars().filter(|c| *c != '-').collect();
    let bytes = hex::decode(&hex).unwrap_or_default();
    if bytes.len() >= 16 {
        let mut buf = [0u8; 16];
        buf.copy_from_slice(&bytes[..16]);
        TraceId::from_bytes(buf)
    } else {
        TraceId::INVALID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_trace_id_uuid() {
        let id = parse_trace_id("550e8400-e29b-41d4-a716-446655440000");
        assert_ne!(id, TraceId::INVALID);
    }

    #[test]
    fn test_parse_trace_id_hex() {
        let id = parse_trace_id("550e8400e29b41d4a716446655440000");
        assert_ne!(id, TraceId::INVALID);
    }

    #[test]
    fn test_parse_trace_id_invalid() {
        let id = parse_trace_id("not-a-valid-id");
        assert_eq!(id, TraceId::INVALID);
    }

    #[test]
    fn test_otel_export_handle_disabled() {
        let handle = OtelExportHandle::disabled();
        // Should not panic on disabled handle
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            handle.export_spans(&[]).await;
            handle.shutdown().await;
        });
    }
}
