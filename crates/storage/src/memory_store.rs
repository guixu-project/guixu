// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::agent::memory::{AgentMemory, MemoryKey};
use rocksdb::{Options, DB};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct MemoryStore {
    db: Arc<DB>,
}

impl MemoryStore {
    pub fn open(path: &Path) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)?;
        Ok(Self { db: Arc::new(db) })
    }

    pub fn open_in_db(db: Arc<DB>) -> Self {
        Self { db }
    }

    pub fn put(&self, memory: &AgentMemory) -> Result<()> {
        let key = memory.key.to_storage_key();
        let value = serde_json::to_vec(memory)?;
        self.db.put(key.as_bytes(), &value)?;
        Ok(())
    }

    pub fn get(&self, key: &MemoryKey) -> Result<Option<AgentMemory>> {
        let storage_key = key.to_storage_key();
        match self.db.get(storage_key.as_bytes())? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn get_or_create(&self, key: &MemoryKey) -> Result<AgentMemory> {
        Ok(self.get(key)?.unwrap_or_else(|| AgentMemory {
            scope: key.to_scope(),
            key: key.clone(),
            ..Default::default()
        }))
    }

    pub fn delete(&self, key: &MemoryKey) -> Result<()> {
        let storage_key = key.to_storage_key();
        self.db.delete(storage_key.as_bytes())?;
        Ok(())
    }

    pub fn list_by_workspace(&self, workspace_id: &str) -> Result<Vec<AgentMemory>> {
        let prefix = format!("mem:{}:", workspace_id);
        let mut results = vec![];
        let iter = self.db.prefix_iterator(prefix.as_bytes());
        for item in iter {
            let (_, v) = item?;
            if let Ok(m) = serde_json::from_slice::<AgentMemory>(&v) {
                results.push(m);
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use data_core::agent::contracts::HostKind;

    #[test]
    fn test_memory_key_to_storage_key() {
        let global_key = MemoryKey::global(HostKind::OpenClaw);
        assert_eq!(global_key.to_storage_key(), "mem:global:openclaw");

        let workspace_key = MemoryKey::workspace("repo:abc", HostKind::Codex);
        assert_eq!(workspace_key.to_storage_key(), "mem:repo:abc:codex");

        let task_family_key =
            MemoryKey::task_family("repo:abc", HostKind::OpenClaw, "cat-detection");
        assert_eq!(
            task_family_key.to_storage_key(),
            "mem:repo:abc:openclaw:tf:cat-detection"
        );

        let job_key = MemoryKey::job("repo:abc", HostKind::OpenClaw, "job_123");
        assert_eq!(
            job_key.to_storage_key(),
            "mem:repo:abc:openclaw:job:job_123"
        );
    }

    #[test]
    fn test_agent_memory_operations() {
        let mut memory = AgentMemory::default();

        memory.record_successful_mapping(
            "cat image classification",
            "hf:org/cat-dataset",
            "huggingface",
            85.5,
        );

        memory.record_decision(
            "job_123",
            "hf:org/cat-dataset",
            data_core::agent::memory::Decision::Accepted,
            "High relevance score, good license",
        );

        memory.add_segment(
            data_core::agent::memory::SegmentType::SearchResult,
            "Found 10 candidate datasets",
            0.5,
        );

        assert_eq!(memory.successful_mappings.len(), 1);
        assert_eq!(memory.decisions.len(), 1);
        assert_eq!(memory.recent_segments.len(), 1);

        let best = memory.get_best_mapping("cat classification");
        assert!(best.is_some());
        assert_eq!(best.unwrap().dataset_cid, "hf:org/cat-dataset");
    }
}

impl MemoryStore {
    pub fn open(path: &Path) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)?;
        Ok(Self { db: Arc::new(db) })
    }

    pub fn open_in_db(db: Arc<DB>) -> Self {
        Self { db }
    }

    pub fn put(&self, memory: &AgentMemory) -> Result<()> {
        let key = memory.key.to_storage_key();
        let value = serde_json::to_vec(memory)?;
        self.db.put(key.as_bytes(), &value)?;
        Ok(())
    }

    pub fn get(&self, key: &MemoryKey) -> Result<Option<AgentMemory>> {
        let storage_key = key.to_storage_key();
        match self.db.get(storage_key.as_bytes())? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn get_or_create(&self, key: &MemoryKey) -> Result<AgentMemory> {
        Ok(self.get(key)?.unwrap_or_else(|| AgentMemory {
            scope: key.to_scope(),
            key: key.clone(),
            ..Default::default()
        }))
    }

    pub fn delete(&self, key: &MemoryKey) -> Result<()> {
        let storage_key = key.to_storage_key();
        self.db.delete(storage_key.as_bytes())?;
        Ok(())
    }

    pub fn list_by_workspace(&self, workspace_id: &str) -> Result<Vec<AgentMemory>> {
        let prefix = format!("mem:{}:", workspace_id);
        let mut results = vec![];
        let iter = self.db.prefix_iterator(prefix.as_bytes());
        for item in iter {
            let (_, v) = item?;
            if let Ok(m) = serde_json::from_slice::<AgentMemory>(&v) {
                results.push(m);
            }
        }
        Ok(results)
    }
}

impl MemoryKey {
    fn to_scope(&self) -> data_agent::memory::MemoryScope {
        if self.job_id.is_some() {
            data_agent::memory::MemoryScope::Job
        } else if self.task_family.is_some() {
            data_agent::memory::MemoryScope::TaskFamily
        } else if self.workspace_id == "global" {
            data_agent::memory::MemoryScope::Global
        } else {
            data_agent::memory::MemoryScope::Workspace
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use data_agent::memory::MemoryKey;
    use data_core::agent::contracts::HostKind;

    #[test]
    fn test_memory_key_to_storage_key() {
        let global_key = MemoryKey::global(HostKind::OpenClaw);
        assert_eq!(global_key.to_storage_key(), "mem:global:openclaw");

        let workspace_key = MemoryKey::workspace("repo:abc", HostKind::Codex);
        assert_eq!(workspace_key.to_storage_key(), "mem:repo:abc:codex");

        let task_family_key =
            MemoryKey::task_family("repo:abc", HostKind::OpenClaw, "cat-detection");
        assert_eq!(
            task_family_key.to_storage_key(),
            "mem:repo:abc:openclaw:tf:cat-detection"
        );

        let job_key = MemoryKey::job("repo:abc", HostKind::OpenClaw, "job_123");
        assert_eq!(
            job_key.to_storage_key(),
            "mem:repo:abc:openclaw:job:job_123"
        );
    }

    #[test]
    fn test_agent_memory_operations() {
        let mut memory = AgentMemory::default();

        memory.record_successful_mapping(
            "cat image classification",
            "hf:org/cat-dataset",
            "huggingface",
            85.5,
        );

        memory.record_decision(
            "job_123",
            "hf:org/cat-dataset",
            data_agent::memory::Decision::Accepted,
            "High relevance score, good license",
        );

        memory.add_segment(
            data_agent::memory::SegmentType::SearchResult,
            "Found 10 candidate datasets",
            0.5,
        );

        assert_eq!(memory.successful_mappings.len(), 1);
        assert_eq!(memory.decisions.len(), 1);
        assert_eq!(memory.recent_segments.len(), 1);

        let best = memory.get_best_mapping("cat classification");
        assert!(best.is_some());
        assert_eq!(best.unwrap().dataset_cid, "hf:org/cat-dataset");
    }
}
