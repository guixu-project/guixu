use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Structured query profile produced before discovery.
///
/// This is the stable intermediate representation between
/// natural-language input and downstream discovery logic.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryProfile {
    pub raw_query: String,
    pub task_type: Option<String>,
    pub target_entity: Option<String>,
    pub quality_hint: Option<String>,
    pub keywords: Vec<String>,
}

/// Produces a structured profile from a raw user query.
#[async_trait::async_trait]
pub trait QueryProfiler: Send + Sync {
    async fn profile(&self, query: &str) -> Result<QueryProfile>;
}

/// Parses natural language queries into structured intents.
/// Uses a local SLM or rule-based fallback.
pub struct IntentParser;

impl IntentParser {
    /// Produce a structured query profile from raw user input.
    pub async fn profile(&self, query: &str) -> Result<QueryProfile> {
        // TODO(milestone-2): Use local SLM (Phi-3-mini via ONNX) for intent extraction
        // Fallback: simple rule-based extraction for a few stable fields.
        let query_lower = query.to_lowercase();
        let keywords: Vec<String> = query
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .filter(|s| s.len() > 1)
            .filter(|s| !matches!(s.as_str(), "to" | "for" | "the" | "and" | "of" | "in" | "on"))
            .collect();

        let task_type = if query_lower.contains("classifier") || query_lower.contains("classification") {
            Some("classification".to_string())
        } else if query_lower.contains("forecast") || query_lower.contains("prediction") {
            Some("forecasting".to_string())
        } else {
            None
        };

        let quality_hint = if query_lower.contains("high-quality") {
            Some("high-quality".to_string())
        } else {
            None
        };

        let target_entity = if let Some((_, tail)) = query_lower.split_once("detect ") {
            tail.split_whitespace()
                .next()
                .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric() && c != '-').to_string())
                .filter(|s| !s.is_empty())
        } else {
            None
        };

        Ok(QueryProfile {
            raw_query: query.to_string(),
            task_type,
            target_entity,
            quality_hint,
            keywords,
        })
    }

    /// Parse a natural language query into structured intent.
    pub async fn parse(&self, query: &str) -> Result<QueryProfile> {
        self.profile(query).await
    }
}

#[async_trait::async_trait]
impl QueryProfiler for IntentParser {
    async fn profile(&self, query: &str) -> Result<QueryProfile> {
        IntentParser::profile(self, query).await
    }
}
