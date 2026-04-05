// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use serde_json::json;

use data_core::config::NodeConfig;

use crate::server::AppState;

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
    let cid = args
        .get("cid")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing cid — pass the dataset CID from search results"))?;

    let download_dir = NodeConfig::config_dir().join("downloads");
    std::fs::create_dir_all(&download_dir)?;

    // Route by CID prefix / source type.
    if let Some(slug) = cid.strip_prefix("kaggle:") {
        download_kaggle(slug, &download_dir).await
    } else if let Some(repo_id) = cid.strip_prefix("hf:") {
        download_huggingface(repo_id, &download_dir).await
    } else if let Some(ipfs_cid) = cid.strip_prefix("ipfs:") {
        download_ipfs(ipfs_cid, &download_dir).await
    } else if cid.starts_with("guixu-hub:") {
        download_guixu_hub(cid, &download_dir, state).await
    } else if cid.len() == 40 && cid.chars().all(|c| c.is_ascii_hexdigit()) {
        // Looks like a BitTorrent info hash.
        download_bittorrent(cid, state).await
    } else {
        anyhow::bail!(
            "unsupported dataset source for CID '{cid}'. \
             Supported prefixes: kaggle:, hf:, ipfs:, guixu-hub:, or a 40-char BT info hash."
        )
    }
}

async fn download_kaggle(slug: &str, download_dir: &std::path::Path) -> Result<String> {
    let dest = download_dir.join(slug.replace('/', "_"));
    std::fs::create_dir_all(&dest)?;

    let output = tokio::process::Command::new("kaggle")
        .args(["datasets", "download", "-d", slug, "-p"])
        .arg(&dest)
        .args(["--unzip"])
        .output()
        .await
        .context("failed to run `kaggle` CLI — is it installed? (`pip install kaggle`)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("kaggle download failed: {stderr}");
    }

    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloaded",
        "source": "kaggle",
        "slug": slug,
        "path": dest.display().to_string(),
    }))?)
}

async fn download_huggingface(repo_id: &str, download_dir: &std::path::Path) -> Result<String> {
    let dest = download_dir.join(repo_id.replace('/', "_"));
    std::fs::create_dir_all(&dest)?;

    // Try huggingface-cli first, fall back to git clone.
    let output = tokio::process::Command::new("huggingface-cli")
        .args(["download", repo_id, "--local-dir"])
        .arg(&dest)
        .args(["--repo-type", "dataset"])
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            return Ok(serde_json::to_string_pretty(&json!({
                "status": "downloaded",
                "source": "huggingface",
                "repo_id": repo_id,
                "path": dest.display().to_string(),
            }))?);
        }
        _ => {}
    }

    // Fallback: git clone
    let url = format!("https://huggingface.co/datasets/{repo_id}");
    let output = tokio::process::Command::new("git")
        .args(["clone", "--depth", "1", &url])
        .arg(&dest)
        .output()
        .await
        .context("failed to run `git clone` for HuggingFace dataset")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "HuggingFace download failed. Install `huggingface-cli` (`pip install huggingface_hub`) \
             or ensure `git` is available. Error: {stderr}"
        );
    }

    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloaded",
        "source": "huggingface",
        "repo_id": repo_id,
        "path": dest.display().to_string(),
    }))?)
}

async fn download_ipfs(ipfs_cid: &str, download_dir: &std::path::Path) -> Result<String> {
    let dest = download_dir.join(ipfs_cid);
    let url = format!("https://ipfs.io/ipfs/{ipfs_cid}");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let response = client
        .get(&url)
        .send()
        .await
        .context("IPFS gateway request failed")?
        .error_for_status()
        .context("IPFS gateway returned error")?;

    let bytes = response.bytes().await?;
    std::fs::write(&dest, &bytes)?;

    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloaded",
        "source": "ipfs",
        "cid": ipfs_cid,
        "path": dest.display().to_string(),
        "size_bytes": bytes.len(),
    }))?)
}

async fn download_guixu_hub(
    cid: &str,
    download_dir: &std::path::Path,
    state: &AppState,
) -> Result<String> {
    // Check if it's free — if so, download directly; otherwise tell user to use dataset_purchase.
    let metadata = state
        .store
        .get(&data_core::types::DatasetCid(cid.to_string()))?;
    if let Some(meta) = &metadata {
        if !meta.price.is_free() {
            anyhow::bail!(
                "dataset {cid} costs {} {} — use dataset_purchase to buy it first",
                meta.price.amount,
                meta.price.currency
            );
        }
    }

    let listing_id = cid.strip_prefix("guixu-hub:").unwrap_or(cid);
    let base_url =
        std::env::var("GUIXU_HUB_BASE_URL").unwrap_or_else(|_| "https://www.guixu.org".into());
    let url = format!("{base_url}/api/hub/datasets/{listing_id}/download");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Guixu Hub download request failed for {listing_id}"))?
        .error_for_status()
        .with_context(|| format!("Guixu Hub download returned error for {listing_id}"))?;

    let dest = download_dir.join(format!("guixu-hub-{listing_id}"));
    let bytes = response.bytes().await?;
    std::fs::write(&dest, &bytes)?;

    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloaded",
        "source": "guixu_hub",
        "cid": cid,
        "path": dest.display().to_string(),
        "size_bytes": bytes.len(),
    }))?)
}

async fn download_bittorrent(info_hash: &str, state: &AppState) -> Result<String> {
    let engine = state
        .torrent_engine
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("torrent engine not initialized — start node first"))?;

    engine.start_download(info_hash).await?;

    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloading",
        "source": "bittorrent",
        "info_hash": info_hash,
        "note": "Use dataset_bt_stats to check progress."
    }))?)
}
