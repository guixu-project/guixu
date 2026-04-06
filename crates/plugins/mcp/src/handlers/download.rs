// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Unified dataset download handler.
//!
//! Routes by CID prefix to the appropriate download method.
//! Prioritizes sources that support anonymous (no-login) download.

use std::path::Path;

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

    if let Some(slug) = cid.strip_prefix("kaggle:") {
        download_kaggle(slug, &download_dir).await
    } else if let Some(repo_id) = cid.strip_prefix("hf:") {
        download_huggingface(repo_id, &download_dir).await
    } else if let Some(ipfs_cid) = cid.strip_prefix("ipfs:") {
        download_http(
            ipfs_cid,
            &format!("https://ipfs.io/ipfs/{ipfs_cid}"),
            "ipfs",
            &download_dir,
        )
        .await
    } else if cid.starts_with("guixu-hub:") {
        download_guixu_hub(cid, &download_dir, state).await
    } else if let Some(id) = cid.strip_prefix("uci:") {
        download_uci(id, &download_dir).await
    } else if let Some(id) = cid.strip_prefix("openml:") {
        download_openml(id, &download_dir).await
    } else if let Some(id) = cid.strip_prefix("zenodo:") {
        download_zenodo(id, &download_dir).await
    } else if let Some(id) = cid.strip_prefix("figshare:") {
        download_figshare(id, &download_dir).await
    } else if let Some(path) = cid.strip_prefix("commoncrawl:") {
        download_http(
            path,
            &format!("https://data.commoncrawl.org/{path}"),
            "commoncrawl",
            &download_dir,
        )
        .await
    } else if let Some(path) = cid.strip_prefix("openalex:") {
        download_s3_no_sign(path, "openalex", "s3://openalex/", &download_dir).await
    } else if let Some(path) = cid.strip_prefix("aws-open:") {
        // Registry of Open Data on AWS — bucket/key format
        download_s3_no_sign(path, "aws-open-data", "s3://", &download_dir).await
    } else if let Some(id) = cid.strip_prefix("openneuro:") {
        download_openneuro(id, &download_dir).await
    } else if let Some(id) = cid.strip_prefix("physionet:") {
        download_physionet(id, &download_dir).await
    } else if cid.len() == 40 && cid.chars().all(|c| c.is_ascii_hexdigit()) {
        download_bittorrent(cid, state).await
    } else {
        anyhow::bail!(
            "unsupported dataset source for CID '{cid}'. Supported: \
             kaggle:, hf:, ipfs:, guixu-hub:, uci:, openml:, zenodo:, figshare:, \
             commoncrawl:, openalex:, aws-open:, openneuro:, physionet:, or 40-char BT info hash."
        )
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .user_agent("guixu/0.1")
        .build()
        .context("build HTTP client")
}

fn safe_filename(id: &str) -> String {
    id.replace(['/', '\\'], "_")
}

async fn download_to_file(url: &str, dest: &Path) -> Result<u64> {
    let client = http_client()?;
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?
        .error_for_status()
        .with_context(|| format!("HTTP error from: {url}"))?;
    let bytes = resp.bytes().await?;
    let len = bytes.len() as u64;
    std::fs::write(dest, &bytes)?;
    Ok(len)
}

fn ok_json(source: &str, id: &str, path: &Path, size: u64) -> Result<String> {
    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloaded",
        "source": source,
        "id": id,
        "path": path.display().to_string(),
        "size_bytes": size,
    }))?)
}

// ---------------------------------------------------------------------------
// Generic HTTP download (IPFS, Common Crawl, etc.)
// ---------------------------------------------------------------------------

async fn download_http(id: &str, url: &str, source: &str, dir: &Path) -> Result<String> {
    let dest = dir.join(safe_filename(id));
    let size = download_to_file(url, &dest).await?;
    ok_json(source, id, &dest, size)
}

// ---------------------------------------------------------------------------
// Kaggle (requires kaggle CLI + token)
// ---------------------------------------------------------------------------

async fn download_kaggle(slug: &str, dir: &Path) -> Result<String> {
    let dest = dir.join(safe_filename(slug));
    std::fs::create_dir_all(&dest)?;
    let output = tokio::process::Command::new("kaggle")
        .args(["datasets", "download", "-d", slug, "-p"])
        .arg(&dest)
        .args(["--unzip"])
        .output()
        .await
        .context("failed to run `kaggle` CLI — install with `pip install kaggle`")?;
    if !output.status.success() {
        anyhow::bail!(
            "kaggle download failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloaded", "source": "kaggle", "slug": slug,
        "path": dest.display().to_string(),
    }))?)
}

// ---------------------------------------------------------------------------
// HuggingFace — public repos: anonymous HTTP; private/gated: needs token
// ---------------------------------------------------------------------------

async fn download_huggingface(repo_id: &str, dir: &Path) -> Result<String> {
    let dest = dir.join(safe_filename(repo_id));
    std::fs::create_dir_all(&dest)?;

    // Try huggingface-cli (handles auth if configured).
    let cli = tokio::process::Command::new("huggingface-cli")
        .args(["download", repo_id, "--local-dir"])
        .arg(&dest)
        .args(["--repo-type", "dataset"])
        .output()
        .await;
    if let Ok(o) = cli {
        if o.status.success() {
            return Ok(serde_json::to_string_pretty(&json!({
                "status": "downloaded", "source": "huggingface", "repo_id": repo_id,
                "path": dest.display().to_string(),
            }))?);
        }
    }

    // Fallback: git clone (works for public repos without token).
    let url = format!("https://huggingface.co/datasets/{repo_id}");
    let output = tokio::process::Command::new("git")
        .args(["clone", "--depth", "1", &url])
        .arg(&dest)
        .output()
        .await
        .context("git clone failed for HuggingFace dataset")?;
    if !output.status.success() {
        anyhow::bail!(
            "HuggingFace download failed. For public repos, ensure `git` is available. \
             For gated repos, install `huggingface-cli` and login. Error: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloaded", "source": "huggingface", "repo_id": repo_id,
        "path": dest.display().to_string(),
    }))?)
}

// ---------------------------------------------------------------------------
// UCI ML Repository — direct HTTP zip download
// ---------------------------------------------------------------------------

async fn download_uci(id: &str, dir: &Path) -> Result<String> {
    // CID format: uci:{numeric_id} or uci:{numeric_id}/{slug}
    // Zip URL: https://archive.ics.uci.edu/static/public/{id}/{slug}.zip
    let (numeric_id, slug) = if let Some((n, s)) = id.split_once('/') {
        (n, Some(s))
    } else {
        (id, None)
    };

    // If slug is provided, try direct zip download.
    if let Some(slug) = slug {
        let url = format!("https://archive.ics.uci.edu/static/public/{numeric_id}/{slug}.zip");
        let dest = dir.join(format!("uci-{numeric_id}-{slug}.zip"));
        let size = download_to_file(&url, &dest).await?;
        return ok_json("uci", id, &dest, size);
    }

    // No slug — try ucimlrepo Python package.
    let dest_dir = dir.join(format!("uci-{numeric_id}"));
    std::fs::create_dir_all(&dest_dir)?;
    let script = format!(
        "from ucimlrepo import fetch_ucirepo\nimport os,json\n\
         d=fetch_ucirepo(id={numeric_id})\n\
         p=os.path.join(r'{dest}','data.csv')\n\
         d.data.original.to_csv(p,index=False)\n\
         print(json.dumps({{'rows':len(d.data.original),'path':p}}))",
        dest = dest_dir.display(),
    );
    let output = tokio::process::Command::new("python3")
        .args(["-c", &script])
        .output()
        .await;
    if let Ok(o) = &output {
        if o.status.success() {
            let info: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap_or_default();
            return Ok(serde_json::to_string_pretty(&json!({
                "status": "downloaded", "source": "uci", "dataset_id": id,
                "path": dest_dir.display().to_string(),
                "rows": info.get("rows"),
            }))?);
        }
    }

    anyhow::bail!(
        "UCI dataset {id} download failed. Either provide slug (e.g. 'uci:53/iris') \
         or install `ucimlrepo` (`pip install ucimlrepo`)."
    )
}

// ---------------------------------------------------------------------------
// OpenML — API download, no auth required
// ---------------------------------------------------------------------------

async fn download_openml(id: &str, dir: &Path) -> Result<String> {
    // OpenML dataset download: https://www.openml.org/data/download/{file_id}
    // or via API: https://www.openml.org/api/v1/json/data/{id}
    // The data/get endpoint returns the dataset file URL.
    let client = http_client()?;
    let api_url = format!("https://www.openml.org/api/v1/json/data/{id}");
    let meta: serde_json::Value = client
        .get(&api_url)
        .send()
        .await?
        .error_for_status()
        .context("OpenML API error")?
        .json()
        .await?;

    let file_url = meta
        .pointer("/data_set_description/url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("OpenML dataset {id} has no download URL"))?;

    let dest = dir.join(format!("openml-{}", safe_filename(id)));
    let size = download_to_file(file_url, &dest).await?;
    ok_json("openml", id, &dest, size)
}

// ---------------------------------------------------------------------------
// Zenodo — Open Access records, direct file download
// ---------------------------------------------------------------------------

async fn download_zenodo(id: &str, dir: &Path) -> Result<String> {
    let client = http_client()?;
    let api_url = format!("https://zenodo.org/api/records/{id}");
    let record: serde_json::Value = client
        .get(&api_url)
        .send()
        .await?
        .error_for_status()
        .context("Zenodo API error")?
        .json()
        .await?;

    let files = record
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("Zenodo record {id} has no files"))?;

    let dest_dir = dir.join(format!("zenodo-{id}"));
    std::fs::create_dir_all(&dest_dir)?;
    let mut total_size = 0u64;
    let mut downloaded = Vec::new();

    for file in files {
        let link = file
            .pointer("/links/self")
            .and_then(|v| v.as_str())
            .or_else(|| file.get("download").and_then(|v| v.as_str()));
        let filename = file.get("key").and_then(|v| v.as_str()).unwrap_or("file");
        if let Some(url) = link {
            let dest = dest_dir.join(filename);
            let size = download_to_file(url, &dest).await?;
            total_size += size;
            downloaded.push(filename.to_string());
        }
    }

    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloaded", "source": "zenodo", "record_id": id,
        "path": dest_dir.display().to_string(),
        "files": downloaded, "total_size_bytes": total_size,
    }))?)
}

// ---------------------------------------------------------------------------
// Figshare — public articles, direct file download
// ---------------------------------------------------------------------------

async fn download_figshare(id: &str, dir: &Path) -> Result<String> {
    let client = http_client()?;
    let api_url = format!("https://api.figshare.com/v2/articles/{id}/files");
    let files: Vec<serde_json::Value> = client
        .get(&api_url)
        .send()
        .await?
        .error_for_status()
        .context("Figshare API error")?
        .json()
        .await?;

    let dest_dir = dir.join(format!("figshare-{id}"));
    std::fs::create_dir_all(&dest_dir)?;
    let mut total_size = 0u64;
    let mut downloaded = Vec::new();

    for file in &files {
        let url = file.get("download_url").and_then(|v| v.as_str());
        let filename = file.get("name").and_then(|v| v.as_str()).unwrap_or("file");
        if let Some(url) = url {
            let dest = dest_dir.join(filename);
            let size = download_to_file(url, &dest).await?;
            total_size += size;
            downloaded.push(filename.to_string());
        }
    }

    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloaded", "source": "figshare", "article_id": id,
        "path": dest_dir.display().to_string(),
        "files": downloaded, "total_size_bytes": total_size,
    }))?)
}

// ---------------------------------------------------------------------------
// S3 no-sign-request (OpenAlex, AWS Open Data)
// ---------------------------------------------------------------------------

async fn download_s3_no_sign(path: &str, source: &str, prefix: &str, dir: &Path) -> Result<String> {
    let dest = dir.join(format!("{source}-{}", safe_filename(path)));
    std::fs::create_dir_all(&dest)?;
    let s3_uri = format!("{prefix}{path}");
    let output = tokio::process::Command::new("aws")
        .args(["s3", "cp", "--no-sign-request", "--recursive", &s3_uri])
        .arg(&dest)
        .output()
        .await
        .context("failed to run `aws` CLI — install AWS CLI for S3 downloads")?;
    if !output.status.success() {
        anyhow::bail!(
            "{source} S3 download failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloaded", "source": source, "s3_path": s3_uri,
        "path": dest.display().to_string(),
    }))?)
}

// ---------------------------------------------------------------------------
// OpenNeuro — public datasets via HTTP
// ---------------------------------------------------------------------------

async fn download_openneuro(id: &str, dir: &Path) -> Result<String> {
    // OpenNeuro snapshot download via S3-compatible endpoint.
    let dest = dir.join(format!("openneuro-{id}"));
    std::fs::create_dir_all(&dest)?;
    let s3_uri = format!("s3://openneuro.org/{id}");
    let output = tokio::process::Command::new("aws")
        .args([
            "s3",
            "sync",
            "--no-sign-request",
            "--endpoint-url",
            "https://s3.amazonaws.com",
            &s3_uri,
        ])
        .arg(&dest)
        .output()
        .await
        .context("failed to run `aws` CLI for OpenNeuro download")?;
    if !output.status.success() {
        anyhow::bail!(
            "OpenNeuro download failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloaded", "source": "openneuro", "dataset_id": id,
        "path": dest.display().to_string(),
    }))?)
}

// ---------------------------------------------------------------------------
// PhysioNet — Open Access datasets via wget
// ---------------------------------------------------------------------------

async fn download_physionet(id: &str, dir: &Path) -> Result<String> {
    let url = format!("https://physionet.org/files/{id}/");
    let dest = dir.join(format!("physionet-{}", safe_filename(id)));
    std::fs::create_dir_all(&dest)?;
    let output = tokio::process::Command::new("wget")
        .args(["-r", "-N", "-c", "-np", "-nH", "--cut-dirs=2", "-P"])
        .arg(&dest)
        .arg(&url)
        .output()
        .await
        .context("failed to run `wget` for PhysioNet download")?;
    if !output.status.success() {
        anyhow::bail!(
            "PhysioNet download failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloaded", "source": "physionet", "dataset_id": id,
        "path": dest.display().to_string(),
    }))?)
}

// ---------------------------------------------------------------------------
// Guixu Hub
// ---------------------------------------------------------------------------

async fn download_guixu_hub(cid: &str, dir: &Path, state: &AppState) -> Result<String> {
    let metadata = state
        .store
        .get(&data_core::types::DatasetCid(cid.to_string()))?;
    if let Some(meta) = &metadata {
        if !meta.price.is_free() {
            anyhow::bail!(
                "dataset {cid} costs {} {} — use dataset_purchase first",
                meta.price.amount,
                meta.price.currency
            );
        }
    }
    let listing_id = cid.strip_prefix("guixu-hub:").unwrap_or(cid);
    let base =
        std::env::var("GUIXU_HUB_BASE_URL").unwrap_or_else(|_| "https://www.guixu.org".into());
    let url = format!("{base}/api/hub/datasets/{listing_id}/download");
    let dest = dir.join(format!("guixu-hub-{listing_id}"));
    let size = download_to_file(&url, &dest).await?;
    ok_json("guixu_hub", cid, &dest, size)
}

// ---------------------------------------------------------------------------
// BitTorrent
// ---------------------------------------------------------------------------

async fn download_bittorrent(info_hash: &str, state: &AppState) -> Result<String> {
    let engine = state
        .torrent_engine
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("torrent engine not initialized — start node first"))?;
    engine.start_download(info_hash).await?;
    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloading", "source": "bittorrent", "info_hash": info_hash,
        "note": "Use dataset_bt_stats to check progress."
    }))?)
}
