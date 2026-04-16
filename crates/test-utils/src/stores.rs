// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Temporary store helpers that auto-clean on drop.

use data_storage::feedback_store::FeedbackStore;
use data_storage::job_store::JobStore;
use data_storage::memory_store::MemoryStore;
use data_storage::metadata_store::MetadataStore;

/// A bundle of all stores backed by a temporary directory.
pub struct TempStores {
    pub metadata: MetadataStore,
    pub feedback: FeedbackStore,
    pub job: JobStore,
    pub memory: MemoryStore,
    _dir: tempfile::TempDir,
}

impl TempStores {
    /// Open all stores in a fresh temporary directory.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let metadata = MetadataStore::open(&dir.path().join("meta")).unwrap();
        let feedback = FeedbackStore::open(&dir.path().join("feedback")).unwrap();
        let job = JobStore::open(&dir.path().join("jobs")).unwrap();
        let memory = MemoryStore::open(&dir.path().join("memory")).unwrap();
        Self {
            metadata,
            feedback,
            job,
            memory,
            _dir: dir,
        }
    }
}

impl Default for TempStores {
    fn default() -> Self {
        Self::new()
    }
}
