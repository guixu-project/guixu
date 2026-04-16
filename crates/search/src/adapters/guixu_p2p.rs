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
