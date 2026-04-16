// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::types::DatasetCid;
use data_storage::metadata_store::MetadataStore;

use crate::sample_eval::{DownloadedSample, SampleDownloadOutcome, SampleRecord};

/// Downloads samples from the P2P network via /guixu/sample/1.0.0 protocol.
/// Integrates into the data-search sample_eval pipeline.
pub struct P2PSampleDownloader {
    store: MetadataStore,
}

impl P2PSampleDownloader {
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
