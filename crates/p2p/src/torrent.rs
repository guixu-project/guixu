use anyhow::Result;
use data_core::types::{AccessMode, DatasetCid};
use std::path::{Path, PathBuf};

/// BitTorrent v2 engine for dataset distribution.
/// Wraps librqbit with protocol-specific logic for open vs paid modes.
pub struct TorrentEngine {
    download_dir: PathBuf,
}

impl TorrentEngine {
    pub fn new(download_dir: PathBuf) -> Self {
        Self { download_dir }
    }

    /// Create a torrent for a local dataset file.
    /// Returns the BT v2 info hash (Merkle root).
    pub async fn create_torrent(&self, file_path: &Path) -> Result<String> {
        // TODO(milestone-1):
        // 1. Read file → split into 256KB pieces
        // 2. SHA-256 each piece → build Merkle tree
        // 3. Generate BT v2 info dict
        // 4. Return info_hash hex
        Ok(String::new())
    }

    /// Start seeding a dataset.
    pub async fn start_seed(
        &self,
        file_path: &Path,
        info_hash: &str,
        access: AccessMode,
    ) -> Result<()> {
        // TODO(milestone-1):
        // - Open mode: standard BT seeding, any peer can connect
        // - Paid mode: only serve to peers with valid access_token
        //   (verified via libp2p authenticated stream)
        Ok(())
    }

    /// Download a dataset by info hash.
    /// For paid datasets, the buyer's access_token is attached.
    pub async fn download(
        &self,
        info_hash: &str,
        access: AccessMode,
        access_token: Option<String>,
    ) -> Result<PathBuf> {
        // TODO(milestone-1):
        // 1. Resolve peers from DHT / tracker
        // 2. Connect and download pieces
        // 3. Verify Merkle tree
        // 4. If paid mode: do NOT cache pieces (no_cache flag)
        // 5. Return path to assembled file
        Ok(self.download_dir.join(info_hash))
    }

    /// Download only the first N rows for preview (range request).
    pub async fn download_preview(
        &self,
        info_hash: &str,
        num_pieces: usize,
    ) -> Result<Vec<u8>> {
        // TODO(milestone-2): download only first N pieces
        Ok(vec![])
    }
}
