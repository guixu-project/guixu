#[cfg(test)]
mod tests {
    use chrono::Utc;
    use data_core::feedback::{DatasetFeedback, ValueAssessment};
    use data_core::metadata::{DatasetMetadata, Provenance};
    use data_core::types::*;

    use crate::feedback_store::FeedbackStore;
    use crate::metadata_store::MetadataStore;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir()
            .join("guixu-test")
            .join(name)
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn make_metadata(cid: &str) -> DatasetMetadata {
        DatasetMetadata {
            cid: DatasetCid(cid.into()),
            info_hash: String::new(),
            title: format!("Dataset {cid}"),
            description: Some("test".into()),
            tags: vec!["test".into()],
            data_type: DataType::Tabular,
            schema: DatasetSchema {
                columns: vec![],
                row_count: 100,
                size_bytes: 1024,
            },
            stats: None,
            video_meta: None,
            access: AccessMode::Open,
            price: Price::usdc(1.0),
            license: License {
                spdx_id: "MIT".into(),
                commercial_use: true,
                derivative_allowed: true,
            },
            provider: Did("did:test:provider".into()),
            signature: String::new(),
            provenance: Provenance::Original,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            verifiable_credential: None,
        }
    }

    // --- MetadataStore tests ---

    #[test]
    fn metadata_put_and_get() {
        let dir = temp_dir("meta-put-get");
        let store = MetadataStore::open(&dir).unwrap();
        let m = make_metadata("cid-1");
        store.put(&m).unwrap();

        let got = store.get(&DatasetCid("cid-1".into())).unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().title, "Dataset cid-1");
    }

    #[test]
    fn metadata_get_missing_returns_none() {
        let dir = temp_dir("meta-missing");
        let store = MetadataStore::open(&dir).unwrap();
        let got = store.get(&DatasetCid("nonexistent".into())).unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn metadata_list_all() {
        let dir = temp_dir("meta-list");
        let store = MetadataStore::open(&dir).unwrap();
        store.put(&make_metadata("a")).unwrap();
        store.put(&make_metadata("b")).unwrap();

        let all = store.list_all().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn metadata_file_path_roundtrip() {
        let dir = temp_dir("meta-filepath");
        let store = MetadataStore::open(&dir).unwrap();
        let cid = DatasetCid("cid-fp".into());
        let path = std::path::PathBuf::from("/tmp/data.csv");

        store.put_file_path(&cid, &path).unwrap();
        let got = store.get_file_path(&cid).unwrap();
        assert_eq!(got.unwrap(), path);
    }

    // --- FeedbackStore tests ---

    fn make_feedback(cid: &str, id: &str, positive: bool) -> DatasetFeedback {
        DatasetFeedback {
            id: id.into(),
            dataset_cid: DatasetCid(cid.into()),
            agent_did: Did("did:test:agent".into()),
            task_type: "classification".into(),
            task_description: "test task".into(),
            relevance_score: if positive { 0.9 } else { 0.2 },
            quality_rating: if positive { 5 } else { 1 },
            value_assessment: if positive {
                ValueAssessment::Positive
            } else {
                ValueAssessment::Negative
            },
            task_success: positive,
            comment: None,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn feedback_put_and_get() {
        let dir = temp_dir("fb-put-get");
        let store = FeedbackStore::open(&dir).unwrap();
        let fb = make_feedback("cid-1", "fb-1", true);
        store.put(&fb).unwrap();

        let fbs = store.get_for_dataset(&DatasetCid("cid-1".into())).unwrap();
        assert_eq!(fbs.len(), 1);
        assert_eq!(fbs[0].id, "fb-1");
    }

    #[test]
    fn feedback_compute_signal_empty() {
        let dir = temp_dir("fb-signal-empty");
        let store = FeedbackStore::open(&dir).unwrap();
        let signal = store
            .compute_signal(&DatasetCid("cid-none".into()))
            .unwrap();
        assert_eq!(signal.total_reviews, 0);
    }

    #[test]
    fn feedback_compute_signal_mixed() {
        let dir = temp_dir("fb-signal-mixed");
        let store = FeedbackStore::open(&dir).unwrap();
        store.put(&make_feedback("cid-1", "fb-1", true)).unwrap();
        store.put(&make_feedback("cid-1", "fb-2", false)).unwrap();

        let signal = store
            .compute_signal(&DatasetCid("cid-1".into()))
            .unwrap();
        assert_eq!(signal.total_reviews, 2);
        assert!((signal.positive_rate - 0.5).abs() < f64::EPSILON);
        assert!((signal.negative_rate - 0.5).abs() < f64::EPSILON);
    }
}
