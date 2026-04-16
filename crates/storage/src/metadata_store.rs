// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use data_core::types::{AccessGrant, DatasetCid, SeedRecord};
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
                let metadata: DatasetMetadata = serde_json::from_slice(&bytes)?;
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

    // ── Seed record CRUD (prefix: seed:{info_hash}) ──

    /// Persist a seed record so it survives restarts.
    pub fn put_seed(&self, record: &SeedRecord) -> Result<()> {
        let key = format!("seed:{}", record.info_hash);
        let value = serde_json::to_vec(record)?;
        self.db.put(key.as_bytes(), &value)?;
        Ok(())
    }

    /// Retrieve a seed record by info hash.
    pub fn get_seed(&self, info_hash: &str) -> Result<Option<SeedRecord>> {
        let key = format!("seed:{info_hash}");
        match self.db.get(key.as_bytes())? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Delete a seed record (called on unpublish).
    pub fn delete_seed(&self, info_hash: &str) -> Result<()> {
        let key = format!("seed:{info_hash}");
        self.db.delete(key.as_bytes())?;
        Ok(())
    }

    /// List all seed records (used on startup to restore seeding).
    pub fn list_seeds(&self) -> Result<Vec<SeedRecord>> {
        let mut seeds = Vec::new();
        let iter = self.db.prefix_iterator(b"seed:");
        for item in iter {
            let (key, value) = item?;
            if !key.starts_with(b"seed:") {
                break;
            }
            if let Ok(record) = serde_json::from_slice::<SeedRecord>(&value) {
                seeds.push(record);
            }
        }
        Ok(seeds)
    }

    // ── Access grant CRUD (prefix: access:{cid}:{buyer_did}) ──

    /// Persist an access grant for dispute resolution and watermark tracing.
    pub fn put_access_grant(
        &self,
        cid: &DatasetCid,
        buyer_did: &str,
        grant: &AccessGrant,
    ) -> Result<()> {
        let key = format!("access:{}:{}", cid.0, buyer_did);
        let value = serde_json::to_vec(grant)?;
        self.db.put(key.as_bytes(), &value)?;
        Ok(())
    }

    /// Retrieve an access grant.
    pub fn get_access_grant(
        &self,
        cid: &DatasetCid,
        buyer_did: &str,
    ) -> Result<Option<AccessGrant>> {
        let key = format!("access:{}:{}", cid.0, buyer_did);
        match self.db.get(key.as_bytes())? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Mark a dataset as unpublished (sets a flag key).
    pub fn mark_unpublished(&self, cid: &DatasetCid) -> Result<()> {
        let key = format!("unpub:{}", cid.0);
        self.db.put(key.as_bytes(), b"1")?;
        self.metadata_cache.write().unwrap().remove(&cid.0);
        Ok(())
    }

    /// Check if a dataset is marked as unpublished.
    pub fn is_unpublished(&self, cid: &DatasetCid) -> Result<bool> {
        let key = format!("unpub:{}", cid.0);
        Ok(self.db.get(key.as_bytes())?.is_some())
    }

    // ── Hit-count tracking (prefix: hits:{kind}:{cid}) ──

    /// Increment a hit counter for a dataset. `kind` is one of "search", "download", "evaluate".
    pub fn increment_hit(&self, cid: &DatasetCid, kind: &str) -> Result<u64> {
        let key = format!("hits:{kind}:{}", cid.0);
        let current = self.get_hit_count(cid, kind)?;
        let next = current + 1;
        self.db.put(key.as_bytes(), next.to_le_bytes())?;
        Ok(next)
    }

    /// Get the hit count for a dataset and kind.
    pub fn get_hit_count(&self, cid: &DatasetCid, kind: &str) -> Result<u64> {
        let key = format!("hits:{kind}:{}", cid.0);
        match self.db.get(key.as_bytes())? {
            Some(bytes) if bytes.len() == 8 => {
                Ok(u64::from_le_bytes(bytes[..8].try_into().unwrap()))
            }
            _ => Ok(0),
        }
    }

    /// Get total popularity score (sum of all hit kinds) for a dataset.
    pub fn popularity(&self, cid: &DatasetCid) -> Result<u64> {
        let s = self.get_hit_count(cid, "search")?;
        let d = self.get_hit_count(cid, "download")?;
        let e = self.get_hit_count(cid, "evaluate")?;
        Ok(s + d * 3 + e * 5)
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
