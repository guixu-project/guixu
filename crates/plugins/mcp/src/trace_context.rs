// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! W3C TraceContext propagation for HTTP headers.
//!
//! Supports inject/extract for MCP HTTP server calls.

use axum::http::HeaderMap;
use data_storage::trace_manager::TraceContext;

/// Header name for the W3C traceparent field.
pub const TRACEPARENT_HEADER: &str = "traceparent";

/// Inject trace context into HTTP headers for outgoing requests.
pub fn inject_trace_context(headers: &mut HeaderMap, context: &TraceContext) {
    let value = context.to_traceparent();
    if let Ok(val) = value.parse() {
        headers.insert(TRACEPARENT_HEADER, val);
    }
}

/// Extract trace context from HTTP headers for incoming requests.
pub fn extract_trace_context(headers: &HeaderMap) -> Option<TraceContext> {
    let header_value = headers.get(TRACEPARENT_HEADER)?;
    let value_str = header_value.to_str().ok()?;
    TraceContext::from_traceparent(value_str)
}
