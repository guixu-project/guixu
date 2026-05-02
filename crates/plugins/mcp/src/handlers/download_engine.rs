// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::Duration;

use data_core::types::{DownloadProgress, DownloadStatus};

const DEFAULT_CONNECTIONS: usize = 4;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadState {
    Idle,
    Resolving,
    Resolved,
    Downloading,
    Paused,
    Done,
    Error,
}

impl DownloadState {
    #[allow(dead_code)]
    #[allow(clippy::wrong_self_convention)]
    fn to_status(&self) -> DownloadStatus {
        match self {
            DownloadState::Idle => DownloadStatus::Ready,
            DownloadState::Resolving => DownloadStatus::Ready,
            DownloadState::Resolved => DownloadStatus::Ready,
            DownloadState::Downloading => DownloadStatus::Running,
            DownloadState::Paused => DownloadStatus::Paused,
            DownloadState::Done => DownloadStatus::Completed,
            DownloadState::Error => DownloadStatus::Failed,
        }
    }

    #[allow(dead_code)]
    fn from_i64(v: i64) -> Self {
        match v {
            0 => DownloadState::Idle,
            1 => DownloadState::Resolving,
            2 => DownloadState::Resolved,
            3 => DownloadState::Downloading,
            4 => DownloadState::Paused,
            5 => DownloadState::Done,
            6 => DownloadState::Error,
            _ => DownloadState::Idle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum ConnectionState {
    NotStarted,
    Connecting,
    Downloading,
    Completed,
    Failed,
}

#[derive(Clone)]
#[allow(dead_code)]
struct Chunk {
    begin: u64,
    end: u64,
    downloaded: u64,
}

#[allow(dead_code)]
impl Chunk {
    fn remain(&self) -> u64 {
        self.end - self.begin + 1 - self.downloaded
    }

    fn new(begin: u64, end: u64) -> Self {
        Self {
            begin,
            end,
            downloaded: 0,
        }
    }
}

#[derive(Clone)]
#[allow(dead_code)]
struct Connection {
    id: usize,
    chunk: Chunk,
    downloaded: u64,
    completed: bool,
    failed: bool,
    retry_times: u32,
    state: ConnectionState,
}

#[allow(dead_code)]
impl Connection {
    fn new(id: usize, chunk: Chunk) -> Self {
        Self {
            id,
            chunk,
            downloaded: 0,
            completed: false,
            failed: false,
            retry_times: 0,
            state: ConnectionState::NotStarted,
        }
    }
}

pub struct HttpDownloader {
    url: String,
    #[allow(dead_code)]
    job_id: String,
    dest_path: Arc<RwLock<Option<std::path::PathBuf>>>,
    total_size: Arc<AtomicU64>,
    downloaded: Arc<AtomicU64>,
    state: Arc<AtomicI64>,
    #[allow(dead_code)]
    connections: Arc<RwLock<Vec<Connection>>>,
    cancel_tx: Arc<RwLock<Option<mpsc::Sender<()>>>>,
    progress_tx: mpsc::Sender<DownloadProgress>,
}

impl HttpDownloader {
    #[allow(dead_code)]
    pub fn new(url: String, job_id: String, progress_tx: mpsc::Sender<DownloadProgress>) -> Self {
        Self {
            url,
            job_id,
            dest_path: Arc::new(RwLock::new(None)),
            total_size: Arc::new(AtomicU64::new(0)),
            downloaded: Arc::new(AtomicU64::new(0)),
            state: Arc::new(AtomicI64::new(DownloadState::Idle as i64)),
            connections: Arc::new(RwLock::new(Vec::new())),
            cancel_tx: Arc::new(RwLock::new(None)),
            progress_tx,
        }
    }

    #[allow(dead_code)]
    fn get_state(&self) -> DownloadState {
        DownloadState::from_i64(self.state.load(Ordering::SeqCst))
    }

    fn set_state(&self, state: DownloadState) {
        self.state.store(state as i64, Ordering::SeqCst);
    }

    pub async fn resolve(&self) -> Result<(u64, bool)> {
        self.set_state(DownloadState::Resolving);
        let client = Client::builder()
            .timeout(CONNECT_TIMEOUT)
            .build()
            .context("failed to build HTTP client")?;

        let resp = client
            .head(&self.url)
            .send()
            .await
            .with_context(|| format!("HEAD request failed for {}", self.url))?;

        if resp.status() != reqwest::StatusCode::OK {
            anyhow::bail!("HEAD request failed with status: {}", resp.status());
        }

        let total_size = resp.content_length().unwrap_or(0);

        let accept_ranges = resp
            .headers()
            .get("accept-ranges")
            .and_then(|v| v.to_str().ok())
            .map(|s| s == "bytes")
            .unwrap_or(false);

        self.total_size.store(total_size, Ordering::SeqCst);
        self.set_state(DownloadState::Resolved);

        Ok((total_size, accept_ranges))
    }

    #[allow(dead_code)]
    pub async fn start(&self, file_path: std::path::PathBuf) -> Result<()> {
        {
            let mut dest = self.dest_path.write().await;
            *dest = Some(file_path);
        }

        let (cancel_tx, mut cancel_rx) = mpsc::channel::<()>(1);
        {
            let mut tx = self.cancel_tx.write().await;
            *tx = Some(cancel_tx);
        }

        self.set_state(DownloadState::Downloading);

        let total_size = self.total_size.load(Ordering::SeqCst);

        if total_size == 0 {
            return self.download_no_range().await;
        }

        let range_supported = self.check_range_support().await?;

        if !range_supported {
            return self.download_no_range().await;
        }

        self.download_with_range(total_size, &mut cancel_rx).await
    }

    async fn check_range_support(&self) -> Result<bool> {
        let client = Client::builder()
            .timeout(CONNECT_TIMEOUT)
            .build()
            .context("failed to build HTTP client")?;

        let resp = client
            .get(&self.url)
            .header("Range", "bytes=0-0")
            .send()
            .await
            .with_context(|| format!("Range probe request failed for {}", self.url))?;

        let status = resp.status();
        let supports_range = status == reqwest::StatusCode::PARTIAL_CONTENT;

        if status != reqwest::StatusCode::OK && !supports_range {
            anyhow::bail!("Range request failed with status: {}", status);
        }

        Ok(supports_range)
    }

    async fn download_no_range(&self) -> Result<()> {
        let dest_path = {
            let guard = self.dest_path.read().await;
            guard.clone().context("dest_path not set")?
        };

        let client = Client::builder()
            .timeout(READ_TIMEOUT)
            .build()
            .context("failed to build HTTP client")?;

        let resp = client
            .get(&self.url)
            .send()
            .await
            .with_context(|| format!("GET request failed for {}", self.url))?;

        if resp.status() != reqwest::StatusCode::OK {
            anyhow::bail!("GET request failed with status: {}", resp.status());
        }

        let mut file = tokio::fs::File::create(&dest_path)
            .await
            .context("failed to create file")?;
        let mut stream = resp.bytes_stream();

        use tokio::io::AsyncWriteExt;
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            self.downloaded
                .fetch_add(chunk.len() as u64, Ordering::SeqCst);
            self.send_progress().await;
        }

        file.flush().await?;
        self.set_state(DownloadState::Done);
        self.send_progress().await;
        Ok(())
    }

    async fn download_with_range(
        &self,
        total_size: u64,
        _cancel_rx: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        let dest_path = {
            let guard = self.dest_path.read().await;
            guard.clone().context("dest_path not set")?
        };

        let _file = tokio::fs::File::create(&dest_path)
            .await
            .context("failed to create file")?;

        let chunk_size = (total_size / DEFAULT_CONNECTIONS as u64).max(1024 * 1024);
        let mut conn_chunks = Vec::new();

        let mut begin = 0u64;
        for i in 0..DEFAULT_CONNECTIONS {
            let end = if i == DEFAULT_CONNECTIONS - 1 {
                total_size.saturating_sub(1)
            } else {
                (begin + chunk_size - 1).min(total_size.saturating_sub(1))
            };
            if begin < total_size {
                conn_chunks.push((i, begin, end));
            }
            begin += chunk_size;
            if begin >= total_size {
                break;
            }
        }

        let url = self.url.clone();
        let downloaded = self.downloaded.clone();

        let handles: Vec<_> = conn_chunks
            .into_iter()
            .map(|(_id, begin, end)| {
                let url = url.clone();
                let downloaded = downloaded.clone();
                tokio::spawn(async move {
                    let client = Client::builder()
                        .timeout(READ_TIMEOUT)
                        .connect_timeout(CONNECT_TIMEOUT)
                        .build()
                        .expect("failed to build HTTP client");
                    let _ = Self::download_range_conn(&client, &url, begin, end, downloaded).await;
                })
            })
            .collect();

        for h in handles {
            let _ = h.await;
        }

        let all_done = self.check_all_done().await;
        if all_done {
            self.set_state(DownloadState::Done);
        } else {
            self.set_state(DownloadState::Error);
        }
        self.send_progress().await;
        Ok(())
    }

    async fn download_range_conn(
        client: &Client,
        url: &str,
        begin: u64,
        end: u64,
        downloaded: Arc<AtomicU64>,
    ) -> Result<()> {
        let resp = client
            .get(url)
            .header("Range", format!("bytes={}-{}", begin, end))
            .send()
            .await
            .with_context(|| format!("Range request failed for {}", url))?;

        if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            anyhow::bail!("Range request failed with status: {}", resp.status());
        }

        let mut stream = resp.bytes_stream();

        loop {
            match stream.next().await {
                Some(Ok(chunk)) => {
                    downloaded.fetch_add(chunk.len() as u64, Ordering::SeqCst);
                }
                Some(Err(e)) => {
                    tracing::error!("download error: {}", e);
                    break;
                }
                None => break,
            }
        }

        Ok(())
    }

    async fn check_all_done(&self) -> bool {
        let downloaded = self.downloaded.load(Ordering::SeqCst);
        let total = self.total_size.load(Ordering::SeqCst);
        total > 0 && downloaded >= total
    }

    #[allow(dead_code)]
    pub async fn pause(&self) {
        if let Some(tx) = self.cancel_tx.read().await.as_ref() {
            let _ = tx.send(()).await;
        }
    }

    #[allow(dead_code)]
    pub async fn get_progress(&self) -> DownloadProgress {
        DownloadProgress {
            downloaded_bytes: self.downloaded.load(Ordering::SeqCst),
            total_bytes: self.total_size.load(Ordering::SeqCst),
            speed_bps: 0,
            connections: 0,
            seed_ratio: None,
        }
    }

    async fn send_progress(&self) {
        let progress = self.get_progress().await;
        let _ = self.progress_tx.send(progress).await;
    }

    #[allow(dead_code)]
    pub fn get_state_atomic(&self) -> Arc<AtomicI64> {
        self.state.clone()
    }
}

#[allow(dead_code)]
pub struct DownloadEngine {
    jobs: Arc<RwLock<std::collections::HashMap<String, Arc<HttpDownloader>>>>,
}

impl DownloadEngine {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    #[allow(dead_code)]
    pub async fn start_download(
        &self,
        job_id: String,
        url: String,
        dest_path: std::path::PathBuf,
    ) -> Result<()> {
        let (progress_tx, mut progress_rx) = mpsc::channel::<DownloadProgress>(100);
        let downloader = Arc::new(HttpDownloader::new(
            url.clone(),
            job_id.clone(),
            progress_tx,
        ));

        {
            let mut jobs = self.jobs.write().await;
            jobs.insert(job_id.clone(), downloader.clone());
        }

        let (total_size, range_supported) = downloader.resolve().await?;
        tracing::info!(
            "resolved {} ({} bytes, range_support={})",
            url,
            total_size,
            range_supported
        );

        let handle = tokio::spawn(async move {
            if let Err(e) = downloader.start(dest_path).await {
                tracing::error!("download failed: {}", e);
            }
        });

        tokio::spawn(async move {
            while let Some(progress) = progress_rx.recv().await {
                tracing::debug!(
                    "download progress: {}/{} bytes",
                    progress.downloaded_bytes,
                    progress.total_bytes
                );
            }
            let _ = handle.await;
        });

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn pause_download(&self, job_id: &str) -> Result<()> {
        let jobs = self.jobs.read().await;
        if let Some(downloader) = jobs.get(job_id) {
            downloader.pause().await;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get_status(&self, job_id: &str) -> Result<Option<DownloadProgress>> {
        let jobs = self.jobs.read().await;
        if let Some(downloader) = jobs.get(job_id) {
            let progress = downloader.get_progress().await;
            Ok(Some(progress))
        } else {
            Ok(None)
        }
    }
}

impl Default for DownloadEngine {
    fn default() -> Self {
        Self::new()
    }
}
