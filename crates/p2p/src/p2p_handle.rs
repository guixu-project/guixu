// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Lazy-initialized P2P subsystem handle.
//!
//! GIP005 requires P2P/Torrent to initialize on-demand, not at startup.
//! This module provides a `P2PHandle` wrapper that defers the actual creation of TorrentEngine.

use anyhow::{Context, Result};
use data_storage::metadata_store::MetadataStore;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// P2P subsystem handle with lazy initialization.
///
/// Initializing the BitTorrent engine eagerly consumes file descriptors and opens
/// network ports (DHT). In lightweight or agent-only modes, P2P may not be needed
/// at all, so we defer initialization until the first actual use.
pub struct P2PHandle {
    inner: Arc<RwLock<Option<TorrentEngineState>>>,
    enabled: bool,
    download_dir: PathBuf,
}

struct TorrentEngineState {
    engine: crate::torrent::TorrentEngine,
}

impl P2PHandle {
    /// Create a new handle.
    ///
    /// `enabled` comes from `features.p2p_torrent` config.
    /// If `false`, all operations will return an error indicating P2P is disabled.
    pub fn new(enabled: bool, download_dir: PathBuf) -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            enabled,
            download_dir,
        }
    }

    /// Returns true if P2P is enabled in config.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Initialize the P2P subsystem. Call this lazily on first use.
    ///
    /// Idempotent — subsequent calls are no-ops.
    pub async fn ensure_initialized(&self) -> Result<()> {
        if !self.enabled {
            return Err(anyhow::anyhow!("P2P disabled in config"));
        }

        let mut guard = self.inner.write().await;
        if guard.is_none() {
            tracing::info!("initializing P2P subsystem...");
            let engine = crate::torrent::TorrentEngine::new(self.download_dir.clone())
                .await
                .context("torrent engine init failed")?;
            *guard = Some(TorrentEngineState { engine });
            tracing::info!("P2P subsystem ready");
        }
        Ok(())
    }

    /// Download a torrent and auto-seed it.
    ///
    /// Initializes P2P if not already done.
    pub async fn download_and_seed(
        &self,
        info_hash: &str,
        store: &MetadataStore,
    ) -> Result<PathBuf> {
        self.ensure_initialized().await?;
        let guard = self.inner.read().await;
        guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("P2P not initialized"))?
            .engine
            .download_and_seed(info_hash, store)
            .await
    }

    /// Download a preview (partial content) from a torrent.
    ///
    /// Initializes P2P if not already done.
    pub async fn download_preview(&self, info_hash: &str, max_bytes: usize) -> Result<Vec<u8>> {
        self.ensure_initialized().await?;
        let guard = self.inner.read().await;
        guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("P2P not initialized"))?
            .engine
            .download_preview(info_hash, max_bytes)
            .await
    }

    /// Get torrent statistics.
    ///
    /// Returns error if P2P not initialized.
    pub fn get_stats(&self, info_hash: &str) -> Result<serde_json::Value> {
        let guard = self.inner.try_read();
        match guard {
            Ok(guard) => match guard.as_ref() {
                Some(state) => state.engine.get_stats(info_hash),
                None => Err(anyhow::anyhow!(
                    "P2P not initialized; call ensure_initialized first"
                )),
            },
            Err(_) => Err(anyhow::anyhow!(
                "P2P not initialized; call ensure_initialized first"
            )),
        }
    }

    /// Start a torrent download.
    ///
    /// Initializes P2P if not already done.
    pub async fn start_download(&self, info_hash: &str) -> Result<()> {
        self.ensure_initialized().await?;
        let guard = self.inner.read().await;
        guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("P2P not initialized"))?
            .engine
            .start_download(info_hash)
            .await
    }
}
