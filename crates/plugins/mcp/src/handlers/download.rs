// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Unified dataset download handler.
//!
//! Routes by CID prefix to the appropriate download method.
//! Prioritizes sources that support anonymous (no-login) download.
//!
//! Now returns job_id for async tracking, inspired by Gopeed's download task model.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use data_core::types::{DataSource, DatasetCid, DownloadJob, DownloadStatus};
use libp2p::futures::StreamExt;
use serde_json::json;
use sha2::Digest;

use data_core::config::NodeConfig;

use crate::handlers::download_engine::HttpDownloader;
use crate::server::AppState;

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
    let cid = args
        .get("cid")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing cid — pass the dataset CID from search results"))?;

    let source = parse_cid_source(cid)?;
    let mut job = DownloadJob::new(DatasetCid(cid.to_string()), source);
    let job_id = job.job_id;

    let download_dir = NodeConfig::config_dir().join("downloads");
    std::fs::create_dir_all(&download_dir)?;

    job.status = DownloadStatus::Queued;
    state.job_store.put_download_job(&job)?;

    let job_id_clone = job_id;
    let cid_clone = cid.to_string();
    let download_dir_clone = download_dir.clone();
    let job_store = Arc::new(state.job_store.clone());

    let job_store_clone = job_store.clone();
    tokio::spawn(async move {
        if let Err(e) =
            async_download(cid_clone, download_dir_clone, job_id_clone, job_store_clone).await
        {
            let _ = job_store.update_download_failed(&job_id_clone, e.to_string());
        }
    });

    Ok(serde_json::to_string_pretty(&json!({
        "status": "queued",
        "job_id": job_id.to_string(),
        "cid": cid,
        "message": "Download queued. Use download_status to track progress."
    }))?)
}

async fn async_download(
    cid: String,
    download_dir: std::path::PathBuf,
    job_id: uuid::Uuid,
    job_store: Arc<data_storage::job_store::JobStore>,
) -> Result<()> {
    let _ = job_store.update_download_status(&job_id, DownloadStatus::Running);

    let result = match cid.strip_prefix("kaggle:") {
        Some(slug) => download_kaggle_async(slug, &download_dir).await,
        None => {
            if let Some(repo_id) = cid.strip_prefix("hf:") {
                download_huggingface_async(repo_id, &download_dir).await
            } else if let Some(ipfs_cid) = cid.strip_prefix("ipfs:") {
                download_http_with_engine(
                    ipfs_cid,
                    &format!("https://ipfs.io/ipfs/{ipfs_cid}"),
                    &download_dir,
                    &job_id,
                    &job_store,
                )
                .await
            } else if cid.starts_with("guixu.market:") {
                download_guixu_hub_async(&cid, &download_dir).await
            } else if let Some(id) = cid.strip_prefix("uci:") {
                download_uci_async(id, &download_dir).await
            } else if let Some(id) = cid.strip_prefix("openml:") {
                download_openml_async(id, &download_dir).await
            } else if let Some(id) = cid.strip_prefix("zenodo:") {
                download_zenodo_async(id, &download_dir).await
            } else if let Some(id) = cid.strip_prefix("figshare:") {
                download_figshare_async(id, &download_dir).await
            } else if let Some(path) = cid.strip_prefix("commoncrawl:") {
                download_http_with_engine(
                    path,
                    &format!("https://data.commoncrawl.org/{path}"),
                    &download_dir,
                    &job_id,
                    &job_store,
                )
                .await
            } else if let Some(path) = cid.strip_prefix("openalex:") {
                download_s3_async(path, "openalex", "s3://openalex/", &download_dir).await
            } else if let Some(path) = cid.strip_prefix("aws-open:") {
                download_s3_async(path, "aws-open-data", "s3://", &download_dir).await
            } else if let Some(id) = cid.strip_prefix("openneuro:") {
                download_openneuro_async(id, &download_dir).await
            } else if let Some(id) = cid.strip_prefix("physionet:") {
                download_physionet_async(id, &download_dir).await
            } else if cid.len() == 40 && cid.chars().all(|c| c.is_ascii_hexdigit()) {
                download_bittorrent_async(&cid).await
            } else {
                Err(anyhow::anyhow!("unsupported dataset source: {cid}"))
            }
        }
    };

    match result {
        Ok(path) => {
            let _ = job_store.update_download_completed(&job_id, path);
        }
        Err(e) => {
            let _ = job_store.update_download_failed(&job_id, e.to_string());
        }
    }
    Ok(())
}

fn parse_cid_source(cid: &str) -> Result<DataSource> {
    let source = if cid.strip_prefix("kaggle:").is_some() {
        DataSource::Kaggle
    } else if cid.strip_prefix("hf:").is_some() {
        DataSource::HuggingFace
    } else if cid.strip_prefix("ipfs:").is_some() {
        DataSource::Ipfs
    } else if cid.starts_with("guixu.market:") {
        DataSource::GuixuHub
    } else if cid.strip_prefix("uci:").is_some() {
        DataSource::DuckDb
    } else if cid.len() == 40 && cid.chars().all(|c| c.is_ascii_hexdigit()) {
        DataSource::BitTorrent
    } else {
        DataSource::P2p
    };
    Ok(source)
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
    let mut result = String::with_capacity(id.len());
    let bytes = id.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'/' | b'\\' => result.push('_'),
            b'.' if i + 1 < bytes.len() && bytes[i + 1] == b'.' => {
                result.push('_');
                result.push('_');
                tracing::warn!("safe_filename: replaced path traversal sequence in id");
            }
            0 => {
                tracing::warn!("safe_filename: null byte detected in id");
            }
            _ => result.push(b as char),
        }
        i += 1;
    }
    if result.is_empty() {
        "unnamed".to_string()
    } else {
        result
    }
}

const CHUNK_SIZE: usize = 8 * 1024 * 1024; // 8 MB chunks

/// Stream-download a file to disk, writing in chunks to avoid loading the
/// entire body into memory.  Writes to a `.tmp` file first, then atomically
/// renames on success.
///
/// If `checksum` is provided it must be a `sha256:...` string and the file
/// will be verified after download.
async fn download_to_file(url: &str, dest: &Path, checksum: Option<&str>) -> Result<u64> {
    let client = http_client()?;
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?
        .error_for_status()
        .with_context(|| format!("HTTP error from: {url}"))?;

    let total_size: u64 = resp.content_length().unwrap_or(0);
    let tmp_dest = dest.with_extension("tmp");

    std::fs::create_dir_all(dest.parent().unwrap_or(Path::new(".")))?;
    let file = tokio::fs::File::create(&tmp_dest).await?;
    let mut writer = tokio::io::BufWriter::with_capacity(CHUNK_SIZE, file);
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    let mut hasher: Option<sha2::Sha256> = checksum.map(|_| sha2::Digest::new());

    use tokio::io::AsyncWriteExt;
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        if let Some(ref mut h) = hasher {
            h.update(&chunk);
        }
        writer.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
    }
    writer.flush().await?;

    // Atomically rename (.tmp -> dest)
    tokio::fs::rename(&tmp_dest, dest).await?;

    // Verify checksum if provided
    if let Some(cs) = checksum {
        let computed = format!("sha256:{:x}", hasher.unwrap().finalize());
        if computed != cs {
            std::fs::remove_file(dest).ok();
            anyhow::bail!("checksum mismatch: expected {cs}, got {computed}");
        }
    }

    Ok(total_size.max(downloaded))
}

#[allow(dead_code)]
fn ok_json(source: &str, id: &str, path: &Path, size: u64) -> Result<String> {
    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloaded",
        "source": source,
        "id": id,
        "path": path.display().to_string(),
        "size_bytes": size,
    }))?)
}

fn check_cli_available(name: &str) -> Result<(), anyhow::Error> {
    std::process::Command::new(name)
        .arg("--version")
        .status()
        .map_err(|e| anyhow::anyhow!("'{}' is not installed or not in PATH: {}", name, e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Generic HTTP download (IPFS, Common Crawl, etc.)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
async fn download_http(id: &str, url: &str, source: &str, dir: &Path) -> Result<String> {
    let dest = dir.join(safe_filename(id));
    let size = download_to_file(url, &dest, None).await?;
    ok_json(source, id, &dest, size)
}

// ---------------------------------------------------------------------------
// Kaggle (requires kaggle CLI + token)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
async fn download_kaggle(slug: &str, dir: &Path) -> Result<String> {
    check_cli_available("kaggle").context("kaggle CLI is required for Kaggle downloads")?;
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

#[allow(dead_code)]
async fn download_huggingface(repo_id: &str, dir: &Path) -> Result<String> {
    let dest = dir.join(safe_filename(repo_id));
    std::fs::create_dir_all(&dest)?;

    // Try huggingface-cli first (handles auth if configured).
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
    check_cli_available("git").context("git CLI is required for HuggingFace downloads")?;
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

#[allow(dead_code)]
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
        let size = download_to_file(&url, &dest, None).await?;
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

#[allow(dead_code)]
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
    let size = download_to_file(file_url, &dest, None).await?;
    ok_json("openml", id, &dest, size)
}

// ---------------------------------------------------------------------------
// Zenodo — Open Access records, direct file download
// ---------------------------------------------------------------------------

#[allow(dead_code)]
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
            let size = download_to_file(url, &dest, None).await?;
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

#[allow(dead_code)]
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
            let size = download_to_file(url, &dest, None).await?;
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

#[allow(dead_code)]
async fn download_s3_no_sign(path: &str, source: &str, prefix: &str, dir: &Path) -> Result<String> {
    check_cli_available("aws").context("aws CLI is required for S3 downloads")?;
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

#[allow(dead_code)]
async fn download_openneuro(id: &str, dir: &Path) -> Result<String> {
    check_cli_available("aws").context("aws CLI is required for OpenNeuro downloads")?;
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

#[allow(dead_code)]
async fn download_physionet(id: &str, dir: &Path) -> Result<String> {
    check_cli_available("wget").context("wget CLI is required for PhysioNet downloads")?;
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

#[allow(dead_code)]
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
    let listing_id = cid.strip_prefix("guixu.market:").unwrap_or(cid);
    let base = std::env::var("GUIXU_MARKET_BASE_URL").map_err(|_| {
        anyhow::anyhow!("GUIXU_MARKET_BASE_URL environment variable not set — Guixu Hub downloads require explicit configuration")
    })?;
    let url = format!("{base}/api/hub/datasets/{listing_id}/download");
    let dest = dir.join(format!("guixu.market-{listing_id}"));
    let size = download_to_file(&url, &dest, None).await?;
    ok_json("guixu_hub", cid, &dest, size)
}

// ---------------------------------------------------------------------------
// Async download implementations (return PathBuf on success)
// ---------------------------------------------------------------------------

async fn download_kaggle_async(slug: &str, dir: &Path) -> Result<std::path::PathBuf> {
    check_cli_available("kaggle").context("kaggle CLI is required for Kaggle downloads")?;
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
    Ok(dest)
}

async fn download_huggingface_async(repo_id: &str, dir: &Path) -> Result<std::path::PathBuf> {
    let dest = dir.join(safe_filename(repo_id));
    std::fs::create_dir_all(&dest)?;

    let cli = tokio::process::Command::new("huggingface-cli")
        .args(["download", repo_id, "--local-dir"])
        .arg(&dest)
        .args(["--repo-type", "dataset"])
        .output()
        .await;
    if let Ok(o) = cli {
        if o.status.success() {
            return Ok(dest);
        }
    }

    check_cli_available("git").context("git CLI is required for HuggingFace downloads")?;
    let url = format!("https://huggingface.co/datasets/{repo_id}");
    let output = tokio::process::Command::new("git")
        .args(["clone", "--depth", "1", &url])
        .arg(&dest)
        .output()
        .await
        .context("git clone failed for HuggingFace dataset")?;
    if !output.status.success() {
        anyhow::bail!(
            "HuggingFace download failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(dest)
}

async fn download_uci_async(id: &str, dir: &Path) -> Result<std::path::PathBuf> {
    let (numeric_id, slug) = if let Some((n, s)) = id.split_once('/') {
        (n, Some(s))
    } else {
        (id, None)
    };

    if let Some(slug) = slug {
        let url = format!("https://archive.ics.uci.edu/static/public/{numeric_id}/{slug}.zip");
        let dest = dir.join(format!("uci-{numeric_id}-{slug}.zip"));
        download_to_file(&url, &dest, None).await?;
        return Ok(dest);
    }

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
            return Ok(dest_dir);
        }
    }
    anyhow::bail!("UCI dataset {id} download failed")
}

async fn download_openml_async(id: &str, dir: &Path) -> Result<std::path::PathBuf> {
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
    download_to_file(file_url, &dest, None).await?;
    Ok(dest)
}

async fn download_zenodo_async(id: &str, dir: &Path) -> Result<std::path::PathBuf> {
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

    for file in files {
        let link = file
            .pointer("/links/self")
            .and_then(|v| v.as_str())
            .or_else(|| file.get("download").and_then(|v| v.as_str()));
        let filename = file.get("key").and_then(|v| v.as_str()).unwrap_or("file");
        if let Some(url) = link {
            let dest = dest_dir.join(filename);
            download_to_file(url, &dest, None).await?;
        }
    }
    Ok(dest_dir)
}

async fn download_figshare_async(id: &str, dir: &Path) -> Result<std::path::PathBuf> {
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

    for file in &files {
        let url = file.get("download_url").and_then(|v| v.as_str());
        let filename = file.get("name").and_then(|v| v.as_str()).unwrap_or("file");
        if let Some(url) = url {
            let dest = dest_dir.join(filename);
            download_to_file(url, &dest, None).await?;
        }
    }
    Ok(dest_dir)
}

#[allow(dead_code)]
async fn download_http_async(id: &str, url: &str, dir: &Path) -> Result<std::path::PathBuf> {
    let dest = dir.join(safe_filename(id));
    download_to_file(url, &dest, None).await?;
    Ok(dest)
}

async fn download_http_with_engine(
    id: &str,
    url: &str,
    dir: &Path,
    job_id: &uuid::Uuid,
    job_store: &Arc<data_storage::job_store::JobStore>,
) -> Result<std::path::PathBuf> {
    use tokio::sync::mpsc;

    let dest = dir.join(safe_filename(id));
    std::fs::create_dir_all(dir)?;

    let (progress_tx, mut progress_rx) = mpsc::channel::<data_core::types::DownloadProgress>(100);

    let downloader = HttpDownloader::new(url.to_string(), job_id.to_string(), progress_tx);

    let (total_size, range_supported) = downloader.resolve().await?;
    tracing::info!(
        "http download resolved {} ({} bytes, range_support={})",
        url,
        total_size,
        range_supported
    );

    let _ = job_store.update_download_progress(
        job_id,
        data_core::types::DownloadProgress {
            downloaded_bytes: 0,
            total_bytes: total_size,
            speed_bps: 0,
            connections: if range_supported { 4 } else { 1 },
            seed_ratio: None,
        },
    );

    let job_id_clone = *job_id;
    let job_store_clone = job_store.clone();
    let dest_clone = dest.clone();

    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let _ = job_store_clone.update_download_progress(&job_id_clone, progress);
        }
    });

    let handle = tokio::spawn(async move {
        if let Err(e) = downloader.start(dest_clone.clone()).await {
            tracing::error!("download failed: {}", e);
            return Err(e);
        }
        Ok::<(), anyhow::Error>(())
    });

    let _ = handle.await?;

    let _ = job_store.update_download_completed(job_id, dest.clone());

    Ok(dest)
}

async fn download_s3_async(
    path: &str,
    source: &str,
    prefix: &str,
    dir: &Path,
) -> Result<std::path::PathBuf> {
    check_cli_available("aws").context("aws CLI is required for S3 downloads")?;
    let dest = dir.join(format!("{source}-{}", safe_filename(path)));
    std::fs::create_dir_all(&dest)?;
    let s3_uri = format!("{prefix}{path}");
    let output = tokio::process::Command::new("aws")
        .args(["s3", "cp", "--no-sign-request", "--recursive", &s3_uri])
        .arg(&dest)
        .output()
        .await
        .context("failed to run `aws` CLI")?;
    if !output.status.success() {
        anyhow::bail!(
            "{source} S3 download failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(dest)
}

async fn download_openneuro_async(id: &str, dir: &Path) -> Result<std::path::PathBuf> {
    check_cli_available("aws").context("aws CLI is required for OpenNeuro downloads")?;
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
    Ok(dest)
}

async fn download_physionet_async(id: &str, dir: &Path) -> Result<std::path::PathBuf> {
    check_cli_available("wget").context("wget CLI is required for PhysioNet downloads")?;
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
    Ok(dest)
}

async fn download_guixu_hub_async(cid: &str, dir: &Path) -> Result<std::path::PathBuf> {
    let listing_id = cid.strip_prefix("guixu.market:").unwrap_or(cid);
    let base = std::env::var("GUIXU_MARKET_BASE_URL")
        .map_err(|_| anyhow::anyhow!("GUIXU_MARKET_BASE_URL not set"))?;
    let url = format!("{base}/api/hub/datasets/{listing_id}/download");
    let dest = dir.join(format!("guixu.market-{listing_id}"));
    download_to_file(&url, &dest, None).await?;
    Ok(dest)
}

async fn download_bittorrent_async(_info_hash: &str) -> Result<std::path::PathBuf> {
    Err(anyhow::anyhow!(
        "bittorrent downloads require synchronous torrent engine - use dataset_bt_download instead"
    ))
}

// ---------------------------------------------------------------------------
// BitTorrent
// ---------------------------------------------------------------------------

#[allow(dead_code)]
async fn download_bittorrent(info_hash: &str, state: &AppState) -> Result<String> {
    // GIP005: P2PHandle initializes lazily on first use
    state.p2p_handle.start_download(info_hash).await?;
    Ok(serde_json::to_string_pretty(&json!({
        "status": "downloading", "source": "bittorrent", "info_hash": info_hash,
        "note": "Use dataset_bt_stats to check progress."
    }))?)
}
