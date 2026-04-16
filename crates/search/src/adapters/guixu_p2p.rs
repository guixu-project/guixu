// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::types::{DataSource, DatasetCid, SearchResult, SkillCapability, SourceFamily};
use data_storage::metadata_store::MetadataStore;

use crate::adapters::ExternalAdapter;

/// P2P network adapter — searches local MetadataStore (populated via DHT/GossipSub)
/// and provides sample preview via the /guixu/sample/1.0.0 protocol.
pub struct GuixuP2PAdapter {
    store: MetadataStore,
}

impl GuixuP2PAdapter {
    pub fn new(store: MetadataStore) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl ExternalAdapter for GuixuP2PAdapter {
    fn name(&self) -> &str {
        "guixu_p2p"
    }

    fn skill_id(&self) -> &str {
        "guixu_p2p"
    }

    fn source_family(&self) -> SourceFamily {
        SourceFamily::Decentralized
    }

    fn capabilities(&self) -> Vec<SkillCapability> {
        vec![
            SkillCapability::Search,
            SkillCapability::Lookup,
            SkillCapability::Download,
            SkillCapability::SchemaProbe,
            SkillCapability::SamplePreview,
        ]
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let all = self.store.list_all()?;
        let query_lower = query.to_lowercase();
        let tokens: Vec<&str> = query_lower.split_whitespace().collect();

        let mut results: Vec<SearchResult> = all
            .into_iter()
            .filter(|m| {
                let text = format!(
                    "{} {} {}",
                    m.title,
                    m.description.as_deref().unwrap_or(""),
                    m.tags.join(" ")
                )
                .to_lowercase();
                tokens.iter().any(|t| text.contains(t))
            })
            .take(limit)
            .map(|m| SearchResult {
                cid: m.cid,
                title: m.title,
                description: m.description,
                tags: m.tags,
                schema: m.schema,
                quality: None,
                price: m.price,
                license: m.license,
                provider: m.provider,
                source: DataSource::P2p,
                market: None,
                data_type: m.data_type,
                created_at: m.created_at,
                seller_endpoint: None,
                source_attributes: None,
                provider_meta: None,
                governance: None,
            })
            .collect();

        results.truncate(limit);
        Ok(results)
    }

    async fn lookup(&self, id: &str) -> Result<Vec<serde_json::Value>> {
        let cid = DatasetCid(id.to_string());
        match self.store.get(&cid)? {
            Some(m) => Ok(vec![serde_json::to_value(&m)?]),
            None => Ok(vec![]),
        }
    }

    async fn schema_probe(&self, id: &str) -> Result<Vec<serde_json::Value>> {
        let cid = DatasetCid(id.to_string());
        match self.store.get(&cid)? {
            Some(m) => Ok(vec![serde_json::json!({
                "cid": m.cid.0,
                "columns": m.schema.columns,
                "row_count": m.schema.row_count,
                "size_bytes": m.schema.size_bytes,
            })]),
            None => Ok(vec![]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use data_core::metadata::{DatasetMetadata, Provenance};
    use data_core::types::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir()
            .join("guixu-test-p2p-adapter")
            .join(name)
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn make_metadata(cid: &str, title: &str, tags: &[&str]) -> DatasetMetadata {
        DatasetMetadata {
            cid: DatasetCid(cid.into()),
            info_hash: None,
            title: title.into(),
            description: Some(format!("desc for {title}")),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            data_type: DataType::Tabular,
            schema: DatasetSchema {
                columns: vec![],
                row_count: 100,
                size_bytes: 1024,
            },
            stats: None,
            video_meta: None,
            access: AccessMode::Open,
            price: Price::free(),
            license: License {
                spdx_id: "MIT".into(),
                commercial_use: true,
                derivative_allowed: true,
            },
            provider: Did("did:test:p".into()),
            signature: String::new(),
            provenance: Provenance::Original,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            verifiable_credential: None,
            source_attributes: None,
        }
    }

    #[tokio::test]
    async fn search_matches_title() {
        let dir = temp_dir("search-title");
        let store = MetadataStore::open(&dir).unwrap();
        store
            .put(&make_metadata("c1", "sentiment analysis", &["nlp"]))
            .unwrap();
        store
            .put(&make_metadata("c2", "image classification", &["cv"]))
            .unwrap();

        let adapter = GuixuP2PAdapter::new(store);
        let results = adapter.search("sentiment", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].cid.0, "c1");
    }

    #[tokio::test]
    async fn search_matches_tags() {
        let dir = temp_dir("search-tags");
        let store = MetadataStore::open(&dir).unwrap();
        store
            .put(&make_metadata("c1", "data1", &["finance", "stocks"]))
            .unwrap();

        let adapter = GuixuP2PAdapter::new(store);
        let results = adapter.search("finance", 10).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn search_no_match_returns_empty() {
        let dir = temp_dir("search-empty");
        let store = MetadataStore::open(&dir).unwrap();
        store.put(&make_metadata("c1", "data1", &[])).unwrap();

        let adapter = GuixuP2PAdapter::new(store);
        let results = adapter.search("nonexistent_query_xyz", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn search_respects_limit() {
        let dir = temp_dir("search-limit");
        let store = MetadataStore::open(&dir).unwrap();
        for i in 0..10 {
            store
                .put(&make_metadata(
                    &format!("c{i}"),
                    &format!("test {i}"),
                    &["test"],
                ))
                .unwrap();
        }

        let adapter = GuixuP2PAdapter::new(store);
        let results = adapter.search("test", 3).await.unwrap();
        assert!(results.len() <= 3);
    }

    #[tokio::test]
    async fn lookup_existing_returns_value() {
        let dir = temp_dir("lookup-ok");
        let store = MetadataStore::open(&dir).unwrap();
        store.put(&make_metadata("c1", "data1", &[])).unwrap();

        let adapter = GuixuP2PAdapter::new(store);
        let results = adapter.lookup("c1").await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn lookup_missing_returns_empty() {
        let dir = temp_dir("lookup-miss");
        let store = MetadataStore::open(&dir).unwrap();
        let adapter = GuixuP2PAdapter::new(store);
        let results = adapter.lookup("nonexistent").await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn schema_probe_returns_schema() {
        let dir = temp_dir("schema");
        let store = MetadataStore::open(&dir).unwrap();
        store.put(&make_metadata("c1", "data1", &[])).unwrap();

        let adapter = GuixuP2PAdapter::new(store);
        let results = adapter.schema_probe("c1").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["row_count"], 100);
    }

    #[test]
    fn capabilities_include_all_expected() {
        let dir = temp_dir("caps");
        let store = MetadataStore::open(&dir).unwrap();
        let adapter = GuixuP2PAdapter::new(store);
        let caps = adapter.capabilities();
        assert!(caps.contains(&SkillCapability::Search));
        assert!(caps.contains(&SkillCapability::Lookup));
        assert!(caps.contains(&SkillCapability::SchemaProbe));
        assert!(caps.contains(&SkillCapability::SamplePreview));
    }

    #[test]
    fn source_family_is_decentralized() {
        let dir = temp_dir("family");
        let store = MetadataStore::open(&dir).unwrap();
        let adapter = GuixuP2PAdapter::new(store);
        assert_eq!(adapter.source_family(), SourceFamily::Decentralized);
    }
}
