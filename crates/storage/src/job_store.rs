// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use chrono::Utc;
use data_core::agent::contracts::{JobEvent, JobId, JobResult, JobState, JobStatus};
use data_core::types::IngestJob;
use rocksdb::{Options, DB};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct JobStore {
    db: Arc<DB>,
}

impl JobStore {
    pub fn open(path: &Path) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)?;
        Ok(Self { db: Arc::new(db) })
    }

    pub fn open_in_db(db: Arc<DB>) -> Self {
        Self { db }
    }

    pub fn put_status(&self, status: &JobStatus) -> Result<()> {
        let key = format!("job:{}", status.job_id.0);
        let value = serde_json::to_vec(status)?;
        self.db.put(key.as_bytes(), &value)?;
        Ok(())
    }

    pub fn get_status(&self, job_id: &JobId) -> Result<Option<JobStatus>> {
        let key = format!("job:{}", job_id.0);
        match self.db.get(key.as_bytes())? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn put_result(&self, result: &JobResult) -> Result<()> {
        let key = format!("result:{}", result.job_id.0);
        let value = serde_json::to_vec(result)?;
        self.db.put(key.as_bytes(), &value)?;
        Ok(())
    }

    pub fn get_result(&self, job_id: &JobId) -> Result<Option<JobResult>> {
        let key = format!("result:{}", job_id.0);
        match self.db.get(key.as_bytes())? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn create_job(&self, job_id: JobId, state: JobState) -> Result<JobStatus> {
        let status = JobStatus {
            job_id: job_id.clone(),
            state,
            updated_at: Utc::now(),
        };
        self.put_status(&status)?;
        Ok(status)
    }

    pub fn update_state(&self, job_id: &JobId, state: JobState) -> Result<JobStatus> {
        let mut status = self
            .get_status(job_id)?
            .ok_or_else(|| anyhow::anyhow!("job not found: {}", job_id.0))?;
        status.state = state;
        status.updated_at = Utc::now();
        self.put_status(&status)?;
        Ok(status)
    }

    pub fn list_jobs(&self) -> Result<Vec<JobStatus>> {
        let mut results = vec![];
        let iter = self.db.prefix_iterator(b"job:");
        for item in iter {
            let (k, v) = item?;
            if !k.starts_with(b"job:") {
                break;
            }
            if let Ok(s) = serde_json::from_slice::<JobStatus>(&v) {
                results.push(s);
            }
        }
        Ok(results)
    }

    pub fn append_event(&self, event: &JobEvent) -> Result<()> {
        let key = format!(
            "event:{}:{}:{}",
            event.job_id.0,
            event.timestamp.timestamp_millis(),
            event.event_id
        );
        let value = serde_json::to_vec(event)?;
        self.db.put(key.as_bytes(), &value)?;
        Ok(())
    }

    pub fn list_events(&self, job_id: &JobId) -> Result<Vec<JobEvent>> {
        let prefix = format!("event:{}:", job_id.0);
        let mut results = vec![];
        let iter = self.db.prefix_iterator(prefix.as_bytes());
        for item in iter {
            let (k, v) = item?;
            if !k.starts_with(prefix.as_bytes()) {
                break;
            }
            if let Ok(event) = serde_json::from_slice::<JobEvent>(&v) {
                results.push(event);
            }
        }
        results.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(results)
    }

    pub fn delete_job(&self, job_id: &JobId) -> Result<()> {
        let key = format!("job:{}", job_id.0);
        self.db.delete(key.as_bytes())?;
        let result_key = format!("result:{}", job_id.0);
        self.db.delete(result_key.as_bytes())?;
        let event_prefix = format!("event:{}:", job_id.0);
        let keys: Result<Vec<Vec<u8>>> = self
            .db
            .prefix_iterator(event_prefix.as_bytes())
            .map(|item| item.map(|(k, _)| k.to_vec()).map_err(Into::into))
            .collect();
        for key in keys? {
            self.db.delete(key)?;
        }
        Ok(())
    }

    // -------------------------------------------------------------------------
    // IngestJob management (download + ingest lifecycle)
    // -------------------------------------------------------------------------

    pub fn put_ingest_job(&self, job: &IngestJob) -> Result<()> {
        let key = format!("ingest:{}", job.job_id);
        let value = serde_json::to_vec(job)?;
        self.db.put(key.as_bytes(), &value)?;
        Ok(())
    }

    pub fn get_ingest_job(&self, job_id: &uuid::Uuid) -> Result<Option<IngestJob>> {
        let key = format!("ingest:{}", job_id);
        match self.db.get(key.as_bytes())? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn update_ingest_state(
        &self,
        job_id: &uuid::Uuid,
        state: data_core::types::IngestState,
    ) -> Result<IngestJob> {
        let mut job = self
            .get_ingest_job(job_id)?
            .ok_or_else(|| anyhow::anyhow!("ingest job not found: {}", job_id))?;
        job.state = state;
        job.updated_at = Utc::now();
        self.put_ingest_job(&job)?;
        Ok(job)
    }

    pub fn list_ingest_jobs(&self) -> Result<Vec<IngestJob>> {
        let mut results = vec![];
        let iter = self.db.prefix_iterator(b"ingest:");
        for item in iter {
            let (k, v) = item?;
            if !k.starts_with(b"ingest:") {
                break;
            }
            if let Ok(job) = serde_json::from_slice::<IngestJob>(&v) {
                results.push(job);
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use data_core::agent::contracts::{JobEventType, WorkerTask, WorkerTaskKind};
    use tempfile::tempdir;

    #[test]
    fn test_job_store_crud() {
        let dir = tempdir().unwrap();
        let store = JobStore::open(dir.path()).unwrap();

        let job_id = JobId::new();
        let status = store.create_job(job_id.clone(), JobState::Queued).unwrap();
        assert_eq!(status.state, JobState::Queued);

        let updated = store.update_state(&job_id, JobState::Running).unwrap();
        assert_eq!(updated.state, JobState::Running);

        let result = store.get_status(&job_id).unwrap().unwrap();
        assert_eq!(result.state, JobState::Running);

        let job_result = JobResult {
            job_id: job_id.clone(),
            selected_dataset: None,
            artifacts: vec![],
            memory_updates: vec![],
            errors: vec![],
        };
        store.put_result(&job_result).unwrap();

        let retrieved_result = store.get_result(&job_id).unwrap().unwrap();
        assert_eq!(retrieved_result.job_id, job_id);

        store.delete_job(&job_id).unwrap();
        assert!(store.get_status(&job_id).unwrap().is_none());
    }

    #[test]
    fn test_job_state_transitions() {
        let dir = tempdir().unwrap();
        let store = JobStore::open(dir.path()).unwrap();

        let job_id = JobId::new();
        store.create_job(job_id.clone(), JobState::Queued).unwrap();

        store.update_state(&job_id, JobState::Running).unwrap();
        store.update_state(&job_id, JobState::Completed).unwrap();

        let status = store.get_status(&job_id).unwrap().unwrap();
        assert_eq!(status.state, JobState::Completed);
    }

    #[test]
    fn test_job_events_roundtrip() {
        let dir = tempdir().unwrap();
        let store = JobStore::open(dir.path()).unwrap();

        let job_id = JobId::new();
        let worker = WorkerTask::new(job_id.clone(), WorkerTaskKind::SearchSkill, "search ipfs");
        let event = JobEvent::new(
            job_id.clone(),
            JobEventType::WorkerStarted,
            "worker started",
            Some(worker.task_id),
            serde_json::json!({ "skill_id": "ipfs" }),
        );

        store.append_event(&event).unwrap();
        let events = store.list_events(&job_id).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, JobEventType::WorkerStarted);
    }
}
