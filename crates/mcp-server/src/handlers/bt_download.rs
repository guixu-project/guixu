use anyhow::Result;
use data_core::types::AccessMode;
use serde_json::json;

use crate::server::AppState;

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
    let info_hash = args
        .get("info_hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing info_hash"))?;

    let engine = state
        .torrent_engine
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("torrent engine not initialized — start node first"))?;

    let path = engine.download(info_hash, AccessMode::Open, None).await?;

    Ok(serde_json::to_string_pretty(&json!({
        "info_hash": info_hash,
        "downloaded_to": path.display().to_string(),
        "status": "completed"
    }))?)
}
