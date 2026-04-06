// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use data_core::types::DatasetCid;
use rocksdb::{Options, DB};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Local persistent storage for metadata and node state (RocksDB).
/// Thread-safe via internal Mutex (RocksDB itself is thread-safe, but we wrap for Arc sharing).
#[derive(Clone)]
pub struct MetadataStore {
    db: Arc<DB>,
    metadata_cache: Arc<RwLock<HashMap<String, DatasetMetadata>>>,
}

impl MetadataStore {
    pub fn open(path: &Path) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)?;
        let db = Arc::new(db);
        let metadata_cache = Arc::new(RwLock::new(load_metadata_cache(&db)?));
        Ok(Self { db, metadata_cache })
    }

    pub fn put(&self, metadata: &DatasetMetadata) -> Result<()> {
        let key = format!("meta:{}", metadata.cid.0);
        let value = serde_json::to_vec(metadata)?;
        self.db.put(key.as_bytes(), &value)?;
        self.metadata_cache
            .write()
            .unwrap()
            .insert(metadata.cid.0.clone(), metadata.clone());
        Ok(())
    }

    pub fn get(&self, cid: &DatasetCid) -> Result<Option<DatasetMetadata>> {
        if let Some(metadata) = self.metadata_cache.read().unwrap().get(&cid.0).cloned() {
            return Ok(Some(metadata));
        }

        let key = format!("meta:{}", cid.0);
        match self.db.get(key.as_bytes())? {
            Some(bytes) => {
                let metadata = serde_json::from_slice(&bytes)?;
                self.metadata_cache
                    .write()
                    .unwrap()
                    .insert(cid.0.clone(), metadata.clone());
                Ok(Some(metadata))
            }
            None => Ok(None),
        }
    }

    pub fn list_all(&self) -> Result<Vec<DatasetMetadata>> {
        Ok(self
            .metadata_cache
            .read()
            .unwrap()
            .values()
            .cloned()
            .collect())
    }

    /// Record the local file path for a dataset CID.
    pub fn put_file_path(&self, cid: &DatasetCid, path: &Path) -> Result<()> {
        let key = format!("file:{}", cid.0);
        self.db
            .put(key.as_bytes(), path.to_string_lossy().as_bytes())?;
        Ok(())
    }

    /// Get the local file path for a dataset CID (if this node published it).
    pub fn get_file_path(&self, cid: &DatasetCid) -> Result<Option<std::path::PathBuf>> {
        let key = format!("file:{}", cid.0);
        match self.db.get(key.as_bytes())? {
            Some(bytes) => Ok(Some(std::path::PathBuf::from(
                String::from_utf8_lossy(&bytes).to_string(),
            ))),
            None => Ok(None),
        }
    }

    /// Record last sync timestamp for an external source.
    pub fn put_sync_state(&self, source: &str, timestamp_secs: u64) -> Result<()> {
        let key = format!("sync:{source}");
        self.db.put(key.as_bytes(), timestamp_secs.to_le_bytes())?;
        Ok(())
    }

    /// Get last sync timestamp for an external source.
    pub fn get_sync_state(&self, source: &str) -> Result<Option<u64>> {
        let key = format!("sync:{source}");
        match self.db.get(key.as_bytes())? {
            Some(bytes) if bytes.len() == 8 => {
                Ok(Some(u64::from_le_bytes(bytes[..8].try_into().unwrap())))
            }
            _ => Ok(None),
        }
    }
}

fn load_metadata_cache(db: &DB) -> Result<HashMap<String, DatasetMetadata>> {
    let mut metadata = HashMap::new();
    let iter = db.prefix_iterator(b"meta:");
    for item in iter {
        let (key, value) = item?;
        if !key.starts_with(b"meta:") {
            break;
        }
        if let Ok(record) = serde_json::from_slice::<DatasetMetadata>(&value) {
            metadata.insert(record.cid.0.clone(), record);
        }
    }
    Ok(metadata)
}
