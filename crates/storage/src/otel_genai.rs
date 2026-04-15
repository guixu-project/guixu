// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! OpenTelemetry GenAI Semantic Conventions (v1.40.0) for Guixu agent tracing.
//!
//! Defines attribute keys, operation names, and provider names per the OTel spec:
//! <https://opentelemetry.io/docs/specs/semconv/gen-ai/>

use opentelemetry::KeyValue;

use crate::trace_store::{SpanRecord, SpanType, TraceSource};

// ---------------------------------------------------------------------------
// gen_ai.* attribute keys (OTel GenAI Semantic Conventions)
// ---------------------------------------------------------------------------

pub const GEN_AI_OPERATION_NAME: &str = "gen_ai.operation.name";
pub const GEN_AI_PROVIDER_NAME: &str = "gen_ai.provider.name";
pub const GEN_AI_REQUEST_MODEL: &str = "gen_ai.request.model";
pub const GEN_AI_RESPONSE_MODEL: &str = "gen_ai.response.model";
pub const GEN_AI_USAGE_INPUT_TOKENS: &str = "gen_ai.usage.input_tokens";
pub const GEN_AI_USAGE_OUTPUT_TOKENS: &str = "gen_ai.usage.output_tokens";
pub const GEN_AI_AGENT_NAME: &str = "gen_ai.agent.name";
pub const GEN_AI_TOOL_NAME: &str = "gen_ai.tool.name";
pub const GEN_AI_TOOL_CALL_ID: &str = "gen_ai.tool.call.id";
pub const GEN_AI_CONVERSATION_ID: &str = "gen_ai.conversation.id";
pub const GEN_AI_RESPONSE_FINISH_REASONS: &str = "gen_ai.response.finish_reasons";
pub const GEN_AI_RESPONSE_ID: &str = "gen_ai.response.id";
pub const GEN_AI_REQUEST_TEMPERATURE: &str = "gen_ai.request.temperature";
pub const GEN_AI_REQUEST_MAX_TOKENS: &str = "gen_ai.request.max_tokens";

// ---------------------------------------------------------------------------
// gen_ai.operation.name well-known values
// ---------------------------------------------------------------------------

pub const OP_CHAT: &str = "chat";
pub const OP_INVOKE_AGENT: &str = "invoke_agent";
pub const OP_EXECUTE_TOOL: &str = "execute_tool";
pub const OP_RETRIEVAL: &str = "retrieval";
pub const OP_EMBEDDINGS: &str = "embeddings";
pub const OP_CREATE_AGENT: &str = "create_agent";

// ---------------------------------------------------------------------------
// gen_ai.provider.name well-known values
// ---------------------------------------------------------------------------

pub const PROVIDER_OPENAI: &str = "openai";
pub const PROVIDER_ANTHROPIC: &str = "anthropic";
pub const PROVIDER_GUIXU: &str = "guixu";

// ---------------------------------------------------------------------------
// error.type attribute
// ---------------------------------------------------------------------------

pub const ERROR_TYPE: &str = "error.type";

// ---------------------------------------------------------------------------
// server.* attributes
// ---------------------------------------------------------------------------

pub const SERVER_ADDRESS: &str = "server.address";

// ---------------------------------------------------------------------------
// Mapping helpers: SpanRecord → OTel KeyValue attributes
// ---------------------------------------------------------------------------

/// Map a Guixu `SpanType` to the corresponding `gen_ai.operation.name`.
pub fn operation_name(span_type: SpanType) -> &'static str {
    match span_type {
        SpanType::Agent => OP_INVOKE_AGENT,
        SpanType::Generation => OP_CHAT,
        SpanType::ToolUse => OP_EXECUTE_TOOL,
        SpanType::Guardrail => OP_EXECUTE_TOOL,
        SpanType::Handoff => OP_INVOKE_AGENT,
        SpanType::User => OP_CHAT,
        SpanType::System => OP_INVOKE_AGENT,
        SpanType::Other => OP_CHAT,
        SpanType::MemoryMutation => OP_INVOKE_AGENT,
    }
}

/// Map a Guixu `TraceSource` to the corresponding `gen_ai.provider.name`.
pub fn provider_name(source: TraceSource) -> &'static str {
    match source {
        TraceSource::Guixu => PROVIDER_GUIXU,
        TraceSource::OpenAi => PROVIDER_OPENAI,
        TraceSource::Claude => PROVIDER_ANTHROPIC,
    }
}

/// Build the OTel span name per GenAI conventions.
///
/// Format: `{operation_name} {model_or_agent_name}`
pub fn span_name(span: &SpanRecord) -> String {
    let op = operation_name(span.span_type);
    let suffix = span
        .model
        .as_deref()
        .or_else(|| {
            span.attributes
                .get(GEN_AI_AGENT_NAME)
                .and_then(|v| v.as_str())
        })
        .unwrap_or(&span.span_name);
    format!("{op} {suffix}")
}

/// Convert a `SpanRecord` into OTel `KeyValue` attributes following GenAI semconv.
pub fn span_attributes(span: &SpanRecord) -> Vec<KeyValue> {
    let mut attrs = vec![
        KeyValue::new(GEN_AI_OPERATION_NAME, operation_name(span.span_type)),
        KeyValue::new(GEN_AI_PROVIDER_NAME, provider_name(span.source)),
    ];

    if let Some(model) = &span.model {
        attrs.push(KeyValue::new(GEN_AI_REQUEST_MODEL, model.clone()));
        attrs.push(KeyValue::new(GEN_AI_RESPONSE_MODEL, model.clone()));
    }

    if let Some(tokens) = span.input_tokens {
        attrs.push(KeyValue::new(GEN_AI_USAGE_INPUT_TOKENS, tokens));
    }
    if let Some(tokens) = span.output_tokens {
        attrs.push(KeyValue::new(GEN_AI_USAGE_OUTPUT_TOKENS, tokens));
    }

    if let Some(err) = &span.error {
        attrs.push(KeyValue::new(ERROR_TYPE, err.clone()));
    }

    // Forward any extra gen_ai.* attributes from the JSON blob.
    if let Some(obj) = span.attributes.as_object() {
        for (k, v) in obj {
            if k.starts_with("gen_ai.") || k.starts_with("server.") {
                if let Some(s) = v.as_str() {
                    attrs.push(KeyValue::new(k.clone(), s.to_string()));
                } else if let Some(n) = v.as_i64() {
                    attrs.push(KeyValue::new(k.clone(), n));
                } else if let Some(f) = v.as_f64() {
                    attrs.push(KeyValue::new(k.clone(), f));
                }
            }
        }
    }

    attrs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace_store::{SpanRecord, SpanType, TraceSource};

    #[test]
    fn test_operation_name_mapping() {
        assert_eq!(operation_name(SpanType::Agent), OP_INVOKE_AGENT);
        assert_eq!(operation_name(SpanType::Generation), OP_CHAT);
        assert_eq!(operation_name(SpanType::ToolUse), OP_EXECUTE_TOOL);
    }

    #[test]
    fn test_provider_name_mapping() {
        assert_eq!(provider_name(TraceSource::OpenAi), PROVIDER_OPENAI);
        assert_eq!(provider_name(TraceSource::Claude), PROVIDER_ANTHROPIC);
        assert_eq!(provider_name(TraceSource::Guixu), PROVIDER_GUIXU);
    }

    #[test]
    fn test_span_attributes_includes_tokens() {
        let span = SpanRecord::new("t1", "s1", None::<String>, "chat", SpanType::Generation)
            .with_model("gpt-4o")
            .with_source(TraceSource::OpenAi)
            .with_input_tokens(100)
            .with_output_tokens(200);

        let attrs = span_attributes(&span);
        let keys: Vec<&str> = attrs.iter().map(|kv| kv.key.as_str()).collect();
        assert!(keys.contains(&GEN_AI_OPERATION_NAME));
        assert!(keys.contains(&GEN_AI_PROVIDER_NAME));
        assert!(keys.contains(&GEN_AI_REQUEST_MODEL));
        assert!(keys.contains(&GEN_AI_USAGE_INPUT_TOKENS));
        assert!(keys.contains(&GEN_AI_USAGE_OUTPUT_TOKENS));
    }

    #[test]
    fn test_span_name_format() {
        let span = SpanRecord::new("t1", "s1", None::<String>, "root", SpanType::Generation)
            .with_model("claude-sonnet-4-20250514");
        assert_eq!(span_name(&span), "chat claude-sonnet-4-20250514");

        let agent_span = SpanRecord::new("t1", "s2", None::<String>, "workflow", SpanType::Agent);
        assert_eq!(span_name(&agent_span), "invoke_agent workflow");
    }
}
