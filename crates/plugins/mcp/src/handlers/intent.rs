use anyhow::Result;

use data_search::intent::IntentParser;

use crate::server::AppState;

pub async fn handle(args: serde_json::Value, _state: &AppState) -> Result<String> {
    let raw_query = args
        .get("raw_query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let query = raw_query
        .or(query)
        .ok_or_else(|| anyhow::anyhow!("missing query"))?;

    let parser = IntentParser::default();
    let profile = parser.profile(query).await?;
    Ok(serde_json::to_string_pretty(&profile)?)
}
