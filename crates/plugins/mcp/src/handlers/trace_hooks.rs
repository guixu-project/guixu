// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Trace hooks for MCP handler execution.
//!
//! Wraps MCP tool handlers with automatic span creation and error recording.

use data_storage::trace_manager::{SpanBuilder, SpanType};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Wrapper that creates a span around an MCP handler execution.
///
/// Automatically derives `parent_span_id` from the current trace context
/// so callers don't need to track it manually.
pub async fn with_trace<F>(
    trace_manager: &Option<Arc<RwLock<data_storage::trace_manager::AgentTraceManager>>>,
    span_name: &str,
    parent_span_id: Option<&str>,
    session_id: Option<&str>,
    f: F,
) -> anyhow::Result<String>
where
    F: std::future::Future<Output = anyhow::Result<String>>,
{
    let trace_info = {
        let Some(tm) = trace_manager else {
            return f.await;
        };
        let tm = tm.read().await;
        if !tm.is_enabled() {
            return f.await;
        }
        // Derive parent from current context if caller didn't provide one
        let effective_parent = match parent_span_id {
            Some(pid) => Some(pid.to_string()),
            None => tm
                .current_context()
                .await
                .map(|ctx| format!("{:016x}", ctx.span_id)),
        };
        let span_id = match tm
            .start_span(effective_parent.as_deref(), span_name, SpanType::ToolUse)
            .await
        {
            Some(id) => id,
            None => return f.await,
        };
        let ctx = tm.current_context().await;
        ctx.map(|c| (span_id, c.trace_id.to_string(), effective_parent))
    };

    let (span_id, trace_id, effective_parent) = match trace_info {
        Some(info) => info,
        None => return f.await,
    };

    let result = f.await;

    // End span after handler completes
    if let Some(tm) = trace_manager {
        let tm = tm.read().await;
        let mut builder = SpanBuilder::new(
            &trace_id,
            &span_id,
            effective_parent.as_deref(),
            span_name,
            SpanType::ToolUse,
        );
        if let Some(sid) = session_id {
            builder = builder.with_session_id(sid);
        }
        if let Err(e) = &result {
            builder = builder.with_error(&e.to_string());
        }
        tm.end_span(builder).await;
    }

    result
}
