use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use data_core::types::DatasetCid;
use rocksdb::{DB, Options};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Local persistent storage for metadata and node state (RocksDB).
/// Thread-safe via internal Mutex (RocksDB itself is thread-safe, but we wrap for Arc sharing).
#[derive(Clone)]
pub struct MetadataStore {
    db: Arc<DB>,
}

impl MetadataStore {
    pub fn open(path: &Path) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)?;
        Ok(Self { db: Arc::new(db) })
    }

    pub fn put(&self, metadata: &DatasetMetadata) -> Result<()> {
        let key = format!("meta:{}", metadata.cid.0);
        let value = serde_json::to_vec(metadata)?;
        self.db.put(key.as_bytes(), &value)?;
        Ok(())
    }

    pub fn get(&self, cid: &DatasetCid) -> Result<Option<DatasetMetadata>> {
        let key = format!("meta:{}", cid.0);
        match self.db.get(key.as_bytes())? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn list_all(&self) -> Result<Vec<DatasetMetadata>> {
        let mut results = vec![];
        let iter = self.db.prefix_iterator(b"meta:");
        for item in iter {
            let (k, v) = item?;
            if !k.starts_with(b"meta:") {
                break;
            }
            if let Ok(m) = serde_json::from_slice::<DatasetMetadata>(&v) {
                results.push(m);
            }
        }
        Ok(results)
    }
}
