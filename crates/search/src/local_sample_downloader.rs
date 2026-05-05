// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::types::DatasetCid;
use data_storage::metadata_store::MetadataStore;

use crate::sample_eval::{DownloadedSample, SampleDownloadOutcome, SampleRecord};

/// Downloads samples from local store.
/// Note: In a full implementation, this would send a SampleRequest via libp2p.
/// Currently reads directly from local MetadataStore.
pub struct LocalSampleDownloader {
    store: MetadataStore,
}

impl LocalSampleDownloader {
    pub fn new(store: MetadataStore) -> Self {
        Self { store }
    }

    /// Attempt to download a sample for the given CID from the local store.
    /// In a full implementation, this would send a SampleRequest via libp2p.
    pub async fn download_sample(
        &self,
        cid: &str,
        max_rows: usize,
    ) -> Result<SampleDownloadOutcome> {
        let dataset_cid = DatasetCid(cid.to_string());

        let file_path = match self.store.get_file_path(&dataset_cid)? {
            Some(p) if p.exists() => p,
            _ => {
                return Ok(SampleDownloadOutcome::unavailable(
                    "dataset not available locally or via P2P",
                ));
            }
        };

        let content = std::fs::read(&file_path)?;
        let text = String::from_utf8_lossy(&content);
        let lines: Vec<&str> = text.lines().take(max_rows + 1).collect();

        if lines.is_empty() {
            return Ok(SampleDownloadOutcome::unavailable("empty file"));
        }

        let records: Vec<SampleRecord> = lines
            .iter()
            .enumerate()
            .map(|(i, line)| SampleRecord {
                id: format!("{i}"),
                content: line.to_string(),
                metadata: serde_json::Value::Null,
            })
            .collect();

        let sampled_bytes = records.iter().map(|r| r.content.len() as u64).sum();
        let sampled_rows = records.len() as u64;

        Ok(SampleDownloadOutcome::available(DownloadedSample {
            records,
            sampled_rows,
            sampled_bytes,
            summary: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir()
            .join("guixu-test-p2p-dl")
            .join(name)
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[tokio::test]
    async fn missing_cid_returns_unavailable() {
        let dir = temp_dir("dl-miss");
        let store = MetadataStore::open(&dir).unwrap();
        let dl = LocalSampleDownloader::new(store);
        let outcome = dl.download_sample("nonexistent", 10).await.unwrap();
        assert!(outcome.sample.is_none());
        assert!(outcome.unavailable_reason.is_some());
    }

    #[tokio::test]
    async fn existing_file_returns_sample() {
        let dir = temp_dir("dl-ok");
        let store = MetadataStore::open(&dir).unwrap();
        let cid = DatasetCid("cid-dl".into());
        let file_path = dir.join("data.csv");
        std::fs::write(&file_path, "a,b\n1,2\n3,4\n").unwrap();
        store.put_file_path(&cid, &file_path).unwrap();

        let dl = LocalSampleDownloader::new(store);
        let outcome = dl.download_sample("cid-dl", 10).await.unwrap();
        assert!(outcome.sample.is_some());
        let sample = outcome.sample.unwrap();
        assert_eq!(sample.records.len(), 3); // header + 2 data rows
        assert!(sample.sampled_bytes > 0);
    }

    #[tokio::test]
    async fn respects_max_rows() {
        let dir = temp_dir("dl-limit");
        let store = MetadataStore::open(&dir).unwrap();
        let cid = DatasetCid("cid-lim".into());
        let file_path = dir.join("big.csv");
        let mut content = "col\n".to_string();
        for i in 0..100 {
            content.push_str(&format!("{i}\n"));
        }
        std::fs::write(&file_path, &content).unwrap();
        store.put_file_path(&cid, &file_path).unwrap();

        let dl = LocalSampleDownloader::new(store);
        let outcome = dl.download_sample("cid-lim", 5).await.unwrap();
        let sample = outcome.sample.unwrap();
        assert!(sample.records.len() <= 6); // max_rows + 1 for header
    }
}
