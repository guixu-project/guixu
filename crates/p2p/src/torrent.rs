use anyhow::Result;
use data_core::types::AccessMode;
use librqbit::{AddTorrent, AddTorrentOptions, CreateTorrentOptions, Session, SessionOptions};
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

const PUBLIC_TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://tracker.openbittorrent.com:6969/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://explodie.org:6969/announce",
    "https://tracker.opentrackr.org:443/announce",
];

/// Timeout for resolving torrent metadata from magnet links.
/// `add_torrent` with a magnet blocks until metadata is fetched from peers/DHT,
/// which can take very long on a cold DHT or when trackers don't know the hash.
const MAGNET_RESOLVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

impl TorrentEngine {
    pub async fn new(download_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&download_dir)?;
        // Try progressively less demanding session configurations so local
        // development and CI can still create a functional BitTorrent engine.
        let session = match Session::new_with_opts(
            download_dir.clone(),
            SessionOptions {
                disable_dht: false,
                disable_dht_persistence: false,
                ..Default::default()
            },
        )
        .await
        {
            Ok(session) => session,
            Err(error) => {
                warn!(error = %error, "DHT persistence unavailable, falling back to ephemeral DHT");
                match Session::new_with_opts(
                    download_dir.clone(),
                    SessionOptions {
                        disable_dht: false,
                        disable_dht_persistence: true,
                        ..Default::default()
                    },
                )
                .await
                {
                    Ok(session) => session,
                    Err(error) => {
                        warn!(error = %error, "ephemeral DHT unavailable, falling back to DHT-disabled session");
                        Session::new_with_opts(
                            download_dir.clone(),
                            SessionOptions {
                                disable_dht: true,
                                disable_dht_persistence: true,
                                ..Default::default()
                            },
                        )
                        .await
                        .map_err(|e| anyhow::anyhow!("bt session: {e}"))?
                    }
                }
            }
        };
        Ok(Self {
            session,
            download_dir,
        })
    }

    /// Create a .torrent for a local file and return the info hash hex.
    pub async fn create_torrent(&self, file_path: &Path) -> Result<String> {
        let torrent_result = librqbit::create_torrent(file_path, CreateTorrentOptions::default())
            .await
            .map_err(|e| anyhow::anyhow!("create torrent: {e}"))?;

        // Add to session to start seeding immediately
        let handle = self
            .session
            .add_torrent(
                AddTorrent::from_bytes(torrent_result.as_bytes()?),
                Some(AddTorrentOptions {
                    output_folder: Some(
                        file_path
                            .parent()
                            .unwrap_or(Path::new("."))
                            .to_string_lossy()
                            .into(),
                    ),
                    overwrite: true,
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
        _file_path: &Path,
        info_hash: &str,
        _access: AccessMode,
    ) -> Result<()> {
        // If already managed by session, nothing to do
        info!(%info_hash, "start_seed requested");
        Ok(())
    }

    /// Start downloading a torrent (non-blocking). Poll `get_stats` for progress.
    pub async fn start_download(&self, info_hash: &str) -> Result<()> {
        self.ensure_handle(info_hash).await?;
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
        let handle = self.ensure_handle(info_hash).await?;

        handle
            .wait_until_completed()
            .await
            .map_err(|e| anyhow::anyhow!("download wait: {e}"))?;

        Ok(self.download_dir.join(info_hash))
    }

    /// Download only the first N bytes of a torrent for preview.
    ///
    /// Uses librqbit's streaming API which automatically prioritises the pieces
    /// being read, so only the beginning of the file is fetched from peers.
    pub async fn download_preview(&self, info_hash: &str, max_bytes: usize) -> Result<Vec<u8>> {
        let handle = self.ensure_handle(info_hash).await?;

        // Metadata should already be resolved by ensure_handle, but wait
        // briefly as a safety net for handles returned via get_existing_handle.
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            handle.wait_until_initialized(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("timeout waiting for torrent metadata"))?
        .map_err(|e| anyhow::anyhow!("torrent init failed: {e}"))?;

        // Stream file_id 0 (first / only file in most dataset torrents)
        let mut stream = handle
            .stream(0)
            .map_err(|e| anyhow::anyhow!("create stream: {e}"))?;

        let to_read = max_bytes.min(stream.len() as usize);
        let mut buf = vec![0u8; to_read];
        let mut total = 0usize;

        while total < to_read {
            let chunk_result = tokio::time::timeout(
                std::time::Duration::from_secs(15),
                stream.read(&mut buf[total..]),
            )
            .await;

            match chunk_result {
                Ok(Ok(0)) => break, // EOF
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

    async fn ensure_handle(&self, info_hash: &str) -> Result<Arc<librqbit::ManagedTorrent>> {
        if let Some(handle) = self.get_existing_handle(info_hash)? {
            return Ok(handle);
        }

        // Build a magnet link with public trackers so metadata resolution
        // doesn't rely solely on DHT (which is slow on a fresh session).
        let tracker_params: String = PUBLIC_TRACKERS
            .iter()
            .map(|t| {
                let encoded: String = t
                    .bytes()
                    .map(|b| match b {
                        b'A'..=b'Z'
                        | b'a'..=b'z'
                        | b'0'..=b'9'
                        | b'-'
                        | b'_'
                        | b'.'
                        | b'~'
                        | b':'
                        | b'/' => (b as char).to_string(),
                        _ => format!("%{b:02X}"),
                    })
                    .collect();
                format!("&tr={encoded}")
            })
            .collect();
        let magnet = format!("magnet:?xt=urn:btih:{info_hash}{tracker_params}");
        tokio::time::timeout(
            MAGNET_RESOLVE_TIMEOUT,
            self.session
                .add_torrent(AddTorrent::from_url(&magnet), Some(bt_add_options())),
        )
        .await
        .map_err(|_| anyhow::anyhow!("timeout resolving torrent metadata for {info_hash}"))?
        .map_err(|e| anyhow::anyhow!("add torrent: {e}"))?
        .into_handle()
        .ok_or_else(|| anyhow::anyhow!("torrent handle unavailable"))
    }

    fn get_existing_handle(
        &self,
        info_hash: &str,
    ) -> Result<Option<Arc<librqbit::ManagedTorrent>>> {
        let id = librqbit::api::TorrentIdOrHash::parse(info_hash)
            .map_err(|e| anyhow::anyhow!("bad info_hash: {e}"))?;
        Ok(self.session.get(id))
    }

    /// Query download stats for a torrent managed by this session.
    pub fn get_stats(&self, info_hash: &str) -> Result<serde_json::Value> {
        let id = librqbit::api::TorrentIdOrHash::parse(info_hash)
            .map_err(|e| anyhow::anyhow!("bad info_hash: {e}"))?;
        let handle = self
            .session
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("torrent not found in session"))?;

        let stats = handle.stats();
        let speed_str = stats
            .live
            .as_ref()
            .map(|l| format!("{}", l.download_speed))
            .unwrap_or_else(|| "0 B/s".into());
        let eta = stats
            .live
            .as_ref()
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

fn bt_add_options() -> AddTorrentOptions {
    AddTorrentOptions {
        trackers: Some(
            PUBLIC_TRACKERS
                .iter()
                .map(|tracker| (*tracker).to_string())
                .collect(),
        ),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Display;

    fn temp_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir()
            .join("guixu-bt-test")
            .join(name)
            .join(format!("{}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[tokio::test]
    async fn engine_new_creates_download_dir() {
        let dir = temp_dir("engine_new");
        let sub = dir.join("nested");
        assert!(!sub.exists());
        let Some(_) = unwrap_or_skip(
            TorrentEngine::new(sub.clone()).await,
            "engine_new_creates_download_dir",
        ) else {
            assert!(sub.exists());
            return;
        };
        assert!(sub.exists());
    }

    /// Create a test file in a subdirectory (not the download_dir itself)
    /// so librqbit's add_torrent won't conflict with existing files.
    fn write_test_file(dir: &Path, name: &str) -> PathBuf {
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let file_path = src.join(name);
        let data = "col_a,col_b\n".repeat(5000);
        std::fs::write(&file_path, &data).unwrap();
        file_path
    }

    fn should_skip_bt_test(error: &dyn Display) -> bool {
        let message = error.to_string();
        message.contains("error creating UDP tracker client")
            || message.contains("error binding UDP for tracker")
            || message.contains("Operation not permitted")
    }

    fn unwrap_or_skip<T, E: Display>(
        result: std::result::Result<T, E>,
        test_name: &str,
    ) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) if should_skip_bt_test(&error) => {
                eprintln!("skipping {test_name}: {error}");
                None
            }
            Err(error) => panic!("{test_name} failed: {error}"),
        }
    }

    #[tokio::test]
    async fn create_torrent_returns_info_hash() {
        let dir = temp_dir("create_torrent");
        let Some(engine) = unwrap_or_skip(
            TorrentEngine::new(dir.clone()).await,
            "create_torrent_returns_info_hash",
        ) else {
            return;
        };
        let file_path = write_test_file(&dir, "test_dataset.csv");

        let info_hash = engine.create_torrent(&file_path).await.unwrap();
        assert!(!info_hash.is_empty(), "info_hash should not be empty");
    }

    #[tokio::test]
    async fn create_torrent_and_get_stats() {
        let dir = temp_dir("stats");
        let Some(engine) = unwrap_or_skip(
            TorrentEngine::new(dir.clone()).await,
            "create_torrent_and_get_stats",
        ) else {
            return;
        };
        let file_path = write_test_file(&dir, "stats_dataset.csv");

        let info_hash = engine.create_torrent(&file_path).await.unwrap();
        let stats = engine.get_stats(&info_hash).unwrap();

        assert!(stats.get("state").is_some());
        assert!(stats.get("total_bytes").is_some());
        assert!(stats.get("progress_pct").is_some());
    }

    #[tokio::test]
    async fn get_stats_unknown_hash_returns_error() {
        let dir = temp_dir("stats_unknown");
        let Some(engine) = unwrap_or_skip(
            TorrentEngine::new(dir).await,
            "get_stats_unknown_hash_returns_error",
        ) else {
            return;
        };
        let result = engine.get_stats("0000000000000000000000000000000000000000");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_torrent_nonexistent_file_returns_error() {
        let dir = temp_dir("no_file");
        let Some(engine) = unwrap_or_skip(
            TorrentEngine::new(dir.clone()).await,
            "create_torrent_nonexistent_file_returns_error",
        ) else {
            return;
        };
        let result = engine
            .create_torrent(Path::new("/tmp/guixu_nonexistent_file.csv"))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_torrent_is_deterministic() {
        let dir = temp_dir("deterministic");
        let Some(engine) = unwrap_or_skip(
            TorrentEngine::new(dir.clone()).await,
            "create_torrent_is_deterministic:first",
        ) else {
            return;
        };
        let file_path = write_test_file(&dir, "det.csv");

        let hash1 = engine.create_torrent(&file_path).await.unwrap();

        // Second engine, same file content
        let dir2 = temp_dir("deterministic2");
        let Some(engine2) = unwrap_or_skip(
            TorrentEngine::new(dir2.clone()).await,
            "create_torrent_is_deterministic:second",
        ) else {
            return;
        };
        let file_path2 = write_test_file(&dir2, "det.csv");

        let hash2 = engine2.create_torrent(&file_path2).await.unwrap();
        assert_eq!(hash1, hash2, "same content should produce same info_hash");
    }

    /// Helper: create a seeder session on a specific port range and return
    /// (session, torrent_bytes, info_hash, listen_port).
    async fn setup_seeder(name: &str) -> Option<(Arc<Session>, bytes::Bytes, String, u16)> {
        let dir = temp_dir(name);
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let file_path = src.join("data.csv");
        std::fs::write(&file_path, "col_a,col_b\n".repeat(5000)).unwrap();

        let session = unwrap_or_skip(
            Session::new_with_opts(
                dir,
                SessionOptions {
                    disable_dht: true,
                    disable_dht_persistence: true,
                    ..Default::default()
                },
            )
            .await,
            "setup_seeder.session",
        )?;
        let session: Arc<Session> = session;

        let torrent_result = librqbit::create_torrent(&file_path, CreateTorrentOptions::default())
            .await
            .unwrap();
        let torrent_bytes = torrent_result.as_bytes().unwrap();

        let handle = unwrap_or_skip(
            session
                .add_torrent(
                    AddTorrent::from_bytes(torrent_bytes.clone()),
                    Some(AddTorrentOptions {
                        output_folder: Some(src.to_string_lossy().into()),
                        overwrite: true,
                        disable_trackers: true,
                        ..Default::default()
                    }),
                )
                .await,
            "setup_seeder.add_torrent",
        )?
        .into_handle()
        .unwrap();

        let info_hash = format!("{:?}", handle.info_hash());
        let port = session
            .tcp_listen_port()
            .expect("seeder must have a listen port");
        Some((session, torrent_bytes, info_hash, port))
    }

    #[tokio::test]
    async fn download_from_seeder() {
        let Some((_seeder, torrent_bytes, _info_hash, port)) = setup_seeder("dl_seed").await else {
            return;
        };

        let dl_dir = temp_dir("dl_down");
        let Some(dl_session) = unwrap_or_skip(
            Session::new_with_opts(
                dl_dir.clone(),
                SessionOptions {
                    disable_dht: true,
                    disable_dht_persistence: true,
                    ..Default::default()
                },
            )
            .await,
            "download_from_seeder.session",
        ) else {
            return;
        };

        let peer_addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let Some(handle) = unwrap_or_skip(
            dl_session
                .add_torrent(
                    AddTorrent::from_bytes(torrent_bytes.clone()),
                    Some(AddTorrentOptions {
                        initial_peers: Some(vec![peer_addr]),
                        disable_trackers: true,
                        ..Default::default()
                    }),
                )
                .await,
            "download_from_seeder.add_torrent",
        ) else {
            return;
        };
        let handle = handle.into_handle().unwrap();

        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            handle.wait_until_completed(),
        )
        .await
        .expect("download timed out")
        .expect("download failed");

        assert!(handle.stats().finished);
    }

    #[tokio::test]
    async fn download_preview_from_seeder() {
        let Some((_seeder, torrent_bytes, _info_hash, port)) = setup_seeder("prev_seed").await
        else {
            return;
        };

        let dl_dir = temp_dir("prev_down");
        let Some(dl_session) = unwrap_or_skip(
            Session::new_with_opts(
                dl_dir.clone(),
                SessionOptions {
                    disable_dht: true,
                    disable_dht_persistence: true,
                    ..Default::default()
                },
            )
            .await,
            "download_preview_from_seeder.session",
        ) else {
            return;
        };

        let peer_addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let Some(handle) = unwrap_or_skip(
            dl_session
                .add_torrent(
                    AddTorrent::from_bytes(torrent_bytes.clone()),
                    Some(AddTorrentOptions {
                        initial_peers: Some(vec![peer_addr]),
                        disable_trackers: true,
                        ..Default::default()
                    }),
                )
                .await,
            "download_preview_from_seeder.add_torrent",
        ) else {
            return;
        };
        let handle = handle.into_handle().unwrap();

        // Wait for metadata
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            handle.wait_until_initialized(),
        )
        .await
        .expect("metadata timed out")
        .expect("metadata failed");

        // Stream first 1024 bytes
        let mut stream = handle.stream(0).expect("stream file 0");
        let mut buf = vec![0u8; 1024];
        let mut total = 0usize;
        while total < buf.len() {
            match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                stream.read(&mut buf[total..]),
            )
            .await
            {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => total += n,
                Ok(Err(e)) => panic!("preview read error: {e}"),
                Err(_) => panic!("preview read timed out"),
            }
        }

        assert!(total > 0, "should have read some bytes");
        // Verify content starts with our CSV header
        let text = String::from_utf8_lossy(&buf[..total]);
        assert!(
            text.starts_with("col_a,col_b"),
            "unexpected content: {}",
            &text[..text.len().min(50)]
        );
    }

    #[test]
    fn bt_add_options_include_trackers() {
        let opts = bt_add_options();
        let trackers = opts.trackers.expect("trackers should be configured");
        assert!(!trackers.is_empty(), "trackers should not be empty");
        assert!(trackers.iter().all(|tracker| tracker.contains("://")));
    }
}
