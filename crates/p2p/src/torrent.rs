use anyhow::Result;
use data_core::types::AccessMode;
use librqbit::{
    AddTorrent, AddTorrentOptions, Session, SessionOptions,
    CreateTorrentOptions,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tracing::{info, warn};

/// BitTorrent engine backed by librqbit.
/// Handles seeding, downloading, and torrent creation for dataset distribution.
pub struct TorrentEngine {
    session: Arc<Session>,
    download_dir: PathBuf,
}

impl TorrentEngine {
    pub async fn new(download_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&download_dir)?;
        let session = Session::new_with_opts(
            download_dir.clone(),
            SessionOptions {
                disable_dht: false,
                disable_dht_persistence: true,
                ..Default::default()
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("bt session: {e}"))?;
        Ok(Self { session: session.into(), download_dir })
    }

    /// Create a .torrent for a local file and return the info hash hex.
    pub async fn create_torrent(&self, file_path: &Path) -> Result<String> {
        let torrent_result = librqbit::create_torrent(
            file_path,
            CreateTorrentOptions::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("create torrent: {e}"))?;

        // Add to session to start seeding immediately
        let handle = self
            .session
            .add_torrent(
                AddTorrent::from_bytes(torrent_result.as_bytes()?),
                Some(AddTorrentOptions {
                    output_folder: Some(
                        file_path.parent().unwrap_or(Path::new(".")).to_string_lossy().into(),
                    ),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| anyhow::anyhow!("add torrent: {e}"))?
            .into_handle()
            .ok_or_else(|| anyhow::anyhow!("torrent already managed"))?;

        let info_hash = format!("{:?}", handle.info_hash());
        info!(%info_hash, file = %file_path.display(), "seeding via BT");
        Ok(info_hash)
    }

    /// Seed an already-known torrent.
    pub async fn start_seed(
        &self,
        file_path: &Path,
        info_hash: &str,
        _access: AccessMode,
    ) -> Result<()> {
        // If already managed by session, nothing to do
        info!(%info_hash, "start_seed requested");
        Ok(())
    }

    /// Start downloading a torrent (non-blocking). Poll `get_stats` for progress.
    pub async fn start_download(&self, info_hash: &str) -> Result<()> {
        let magnet = format!("magnet:?xt=urn:btih:{info_hash}");
        self.session
            .add_torrent(
                AddTorrent::from_url(&magnet),
                Some(AddTorrentOptions::default()),
            )
            .await
            .map_err(|e| anyhow::anyhow!("start download: {e}"))?;
        info!(info_hash, "download started");
        Ok(())
    }

    /// Download a dataset by magnet link or info hash (blocking until complete).
    pub async fn download(
        &self,
        info_hash: &str,
        _access: AccessMode,
        _access_token: Option<String>,
    ) -> Result<PathBuf> {
        let magnet = format!("magnet:?xt=urn:btih:{info_hash}");
        let handle = self
            .session
            .add_torrent(
                AddTorrent::from_url(&magnet),
                Some(AddTorrentOptions::default()),
            )
            .await
            .map_err(|e| anyhow::anyhow!("download torrent: {e}"))?
            .into_handle()
            .ok_or_else(|| anyhow::anyhow!("torrent already managed"))?;

        // Wait for completion
        handle.wait_until_completed().await
            .map_err(|e| anyhow::anyhow!("download wait: {e}"))?;

        Ok(self.download_dir.join(info_hash))
    }

    /// Download only the first N bytes of a torrent for preview.
    ///
    /// Uses librqbit's streaming API which automatically prioritises the pieces
    /// being read, so only the beginning of the file is fetched from peers.
    pub async fn download_preview(
        &self,
        info_hash: &str,
        max_bytes: usize,
    ) -> Result<Vec<u8>> {
        let magnet = format!("magnet:?xt=urn:btih:{info_hash}");
        let handle = self
            .session
            .add_torrent(
                AddTorrent::from_url(&magnet),
                Some(AddTorrentOptions::default()),
            )
            .await
            .map_err(|e| anyhow::anyhow!("add torrent for preview: {e}"))?
            .into_handle()
            .ok_or_else(|| anyhow::anyhow!("torrent already managed"))?;

        // Wait for metadata (torrent info) — not the full download.
        // Timeout: metadata resolution depends on DHT / tracker availability.
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            handle.wait_until_initialized(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("timeout waiting for torrent metadata"))?
        .map_err(|e| anyhow::anyhow!("torrent init failed: {e}"))?;

        // Stream file_id 0 (first / only file in most dataset torrents)
        let mut stream = handle.stream(0)
            .map_err(|e| anyhow::anyhow!("create stream: {e}"))?;

        let to_read = max_bytes.min(stream.len() as usize);
        let mut buf = vec![0u8; to_read];
        let mut total = 0usize;

        // Read with a timeout per chunk to avoid hanging on dead torrents
        while total < to_read {
            let chunk_result = tokio::time::timeout(
                std::time::Duration::from_secs(15),
                stream.read(&mut buf[total..]),
            )
            .await;

            match chunk_result {
                Ok(Ok(0)) => break,           // EOF
                Ok(Ok(n)) => total += n,
                Ok(Err(e)) => {
                    warn!(info_hash, error = %e, bytes_read = total, "preview stream error");
                    break;
                }
                Err(_) => {
                    warn!(info_hash, bytes_read = total, "preview read timeout");
                    break;
                }
            }
        }

        buf.truncate(total);
        info!(info_hash, bytes = total, "preview downloaded");
        Ok(buf)
    }

    /// Query download stats for a torrent managed by this session.
    pub fn get_stats(&self, info_hash: &str) -> Result<serde_json::Value> {
        let id = librqbit::api::TorrentIdOrHash::parse(info_hash)
            .map_err(|e| anyhow::anyhow!("bad info_hash: {e}"))?;
        let handle = self.session.get(id)
            .ok_or_else(|| anyhow::anyhow!("torrent not found in session"))?;

        let stats = handle.stats();
        let speed_str = stats.live.as_ref()
            .map(|l| format!("{}", l.download_speed))
            .unwrap_or_else(|| "0 B/s".into());
        let eta = stats.live.as_ref()
            .and_then(|l| l.time_remaining.as_ref())
            .map(|t| format!("{t}"));

        let pct = if stats.total_bytes > 0 {
            (stats.progress_bytes as f64 / stats.total_bytes as f64) * 100.0
        } else {
            0.0
        };

        Ok(serde_json::json!({
            "state": format!("{}", stats.state),
            "progress_bytes": stats.progress_bytes,
            "total_bytes": stats.total_bytes,
            "progress_pct": format!("{pct:.1}"),
            "download_speed": speed_str,
            "eta": eta,
            "finished": stats.finished,
            "error": stats.error,
        }))
    }
}
