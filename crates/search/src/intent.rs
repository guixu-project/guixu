use anyhow::Result;

use crate::engine::ParsedIntent;

/// Parses natural language queries into structured intents.
/// Uses a local SLM or rule-based fallback.
pub struct IntentParser;

impl IntentParser {
    /// Parse a natural language query into structured intent.
    pub async fn parse(&self, query: &str) -> Result<ParsedIntent> {
        // TODO(milestone-2): Use local SLM (Phi-3-mini via ONNX) for intent extraction
        // Fallback: simple keyword extraction
        let keywords: Vec<String> = query
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .filter(|s| s.len() > 1)
            .collect();

        Ok(ParsedIntent {
            raw_query: query.to_string(),
            topic: None,
            geo: None,
            temporal: None,
            format: None,
            keywords,
        })
    }
}
