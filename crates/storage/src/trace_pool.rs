// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Channel-based DuckDB connection pool.
//!
//! DuckDB `Connection` is **not** `Sync` (it uses `RefCell` internally), so it
//! cannot be shared across threads behind an `Arc`. This pool pre-creates a
//! fixed number of connections and hands them out via an async channel. Each
//! caller receives exclusive ownership of a connection and returns it when done.

use std::path::{Path, PathBuf};

use anyhow::Result;
use tokio::sync::mpsc;

use crate::trace_store::TraceStore;

/// A pool of [`TraceStore`] connections backed by a bounded channel.
pub struct TracePool {
    tx: mpsc::Sender<TraceStore>,
    rx: tokio::sync::Mutex<mpsc::Receiver<TraceStore>>,
    path: PathBuf,
}

impl TracePool {
    /// Create a pool with `size` pre-opened connections to the given DuckDB file.
    pub fn open(path: &Path, size: usize) -> Result<Self> {
        let size = size.max(1);
        let (tx, rx) = mpsc::channel(size);
        for _ in 0..size {
            let store = TraceStore::open(path)?;
            tx.try_send(store).expect("channel has capacity");
        }
        Ok(Self {
            tx,
            rx: tokio::sync::Mutex::new(rx),
            path: path.to_path_buf(),
        })
    }

    /// Take a connection from the pool. The caller is responsible for returning
    /// it via [`sender()`] when done.
    pub async fn get(&self) -> Result<TraceStore> {
        self.rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("trace pool closed"))
    }

    /// Get a clone of the return channel for sending connections back.
    pub fn sender(&self) -> mpsc::Sender<TraceStore> {
        self.tx.clone()
    }

    /// The database path this pool is connected to.
    pub fn path(&self) -> &Path {
        &self.path
    }
}
