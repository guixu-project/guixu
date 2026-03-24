use anyhow::Result;
use data_core::types::AccessMode;
use librqbit::{
    AddTorrent, AddTorrentOptions, Session, SessionOptions,
    CreateTorrentOptions,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

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

    /// Download a dataset by magnet link or info hash.
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

    /// Download only the first N pieces for preview.
    pub async fn download_preview(
        &self,
        info_hash: &str,
        _num_pieces: usize,
    ) -> Result<Vec<u8>> {
        // TODO: partial piece download via librqbit priority API
        Ok(vec![])
    }
}
