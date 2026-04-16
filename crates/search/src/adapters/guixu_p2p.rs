// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::metadata::DatasetMetadata;
use data_core::types::{DataSource, DatasetCid, SearchResult, SkillCapability, SourceFamily};
use data_storage::metadata_store::MetadataStore;
use std::collections::HashSet;
use tracing::debug;

use crate::adapters::ExternalAdapter;
use crate::vector_index::VectorIndex;

/// Optional handle for DHT queries (wraps NetworkHandle's dht_get).
#[async_trait::async_trait]
pub trait DhtQuerier: Send + Sync {
    async fn dht_get(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>>;
}

/// P2P network adapter — searches local MetadataStore (populated via DHT/GossipSub),
/// queries DHT for remote datasets by tag, ranks results by relevance/quality/popularity,
/// and falls back to VectorIndex for semantic search when keyword matches are sparse.
pub struct GuixuP2PAdapter {
    store: MetadataStore,
    dht: Option<Box<dyn DhtQuerier>>,
    vector_index: Option<VectorIndex>,
}

impl GuixuP2PAdapter {
    pub fn new(store: MetadataStore) -> Self {
        Self {
            store,
            dht: None,
            vector_index: None,
        }
    }

    pub fn with_dht(mut self, dht: Box<dyn DhtQuerier>) -> Self {
        self.dht = Some(dht);
        self
    }

    pub fn with_vector_index(mut self, vi: VectorIndex) -> Self {
        self.vector_index = Some(vi);
        self
    }
}

/// Relevance score for a single metadata entry against a query.
fn score_metadata(m: &DatasetMetadata, tokens: &[String], popularity: u64) -> f64 {
    let text = format!(
        "{} {} {}",
        m.title,
        m.description.as_deref().unwrap_or(""),
        m.tags.join(" ")
    )
    .to_lowercase();

    if tokens.is_empty() {
        return 0.0;
    }

    // Keyword match ratio (0..1)
    let hits = tokens.iter().filter(|t| text.contains(t.as_str())).count();
    if hits == 0 {
        return 0.0;
    }
    let keyword_score = hits as f64 / tokens.len() as f64;

    // Title match bonus: tokens in title are worth more
    let title_lower = m.title.to_lowercase();
    let title_hits = tokens
        .iter()
        .filter(|t| title_lower.contains(t.as_str()))
        .count();
    let title_bonus = title_hits as f64 / tokens.len() as f64 * 0.2;

    // Schema completeness (0..1): having columns and row_count > 0
    let schema_score = if !m.schema.columns.is_empty() && m.schema.row_count > 0 {
        1.0
    } else if m.schema.row_count > 0 || !m.schema.columns.is_empty() {
        0.5
    } else {
        0.0
    };

    // Freshness: days since update, decayed
    let age_days = (chrono::Utc::now() - m.updated_at).num_days().max(0) as f64;
    let freshness = 1.0 / (1.0 + age_days / 30.0);

    // Popularity (log scale, capped)
    let pop_score = (1.0 + popularity as f64).ln() / 10.0_f64.ln();
    let pop_score = pop_score.min(1.0);

    // Weighted combination
    0.45 * keyword_score
        + 0.20 * title_bonus
        + 0.15 * schema_score
        + 0.10 * freshness
        + 0.10 * pop_score
}

fn metadata_to_search_result(m: DatasetMetadata) -> SearchResult {
    SearchResult {
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
        let tokens: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect();

        // ── Phase 1: local keyword search with scoring ──
        let all = self.store.list_all()?;
        let mut seen_cids: HashSet<String> = HashSet::new();
        let mut scored: Vec<(f64, DatasetMetadata)> = all
            .into_iter()
            .filter_map(|m| {
                let pop = self.store.popularity(&m.cid).unwrap_or(0);
                let s = score_metadata(&m, &tokens, pop);
                if s > 0.0 {
                    seen_cids.insert(m.cid.0.clone());
                    // Record search hit
                    let _ = self.store.increment_hit(&m.cid, "search");
                    Some((s, m))
                } else {
                    None
                }
            })
            .collect();

        // ── Phase 2: DHT remote search by tags ──
        if let Some(ref dht) = self.dht {
            for token in &tokens {
                // Query DHT for tag:{token}:* — we probe a few known CID patterns
                // In practice, Kademlia prefix queries aren't supported, so we query
                // the tag key directly. The DHT stores tag:{tag}:{cid} → cid_bytes.
                // We can only look up exact tag keys if we know the CID, but we can
                // look up metadata by CID if we discover one.
                // Strategy: query tag:{token} as a prefix hint — if the DHT supports
                // GetRecord with the exact key, we try a few.
                // For now, we query meta: for CIDs discovered via other means.
                let tag_prefix = format!("tag:{token}:");
                // Try to get metadata for CIDs we haven't seen yet by querying
                // a well-known tag index key (this works when DHT stores exact keys)
                let key = tag_prefix.into_bytes();
                if let Ok(Some(cid_bytes)) = dht.dht_get(key).await {
                    if let Ok(cid_str) = String::from_utf8(cid_bytes) {
                        if !seen_cids.contains(&cid_str) {
                            let meta_key = format!("meta:{cid_str}").into_bytes();
                            if let Ok(Some(meta_bytes)) = dht.dht_get(meta_key).await {
                                if let Ok(m) =
                                    serde_json::from_slice::<DatasetMetadata>(&meta_bytes)
                                {
                                    let pop = self.store.popularity(&m.cid).unwrap_or(0);
                                    let s = score_metadata(&m, &tokens, pop);
                                    seen_cids.insert(cid_str);
                                    scored.push((s.max(0.01), m));
                                    debug!(tag = token.as_str(), "DHT tag lookup hit");
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── Phase 3: VectorIndex semantic fallback ──
        if scored.len() < limit {
            if let Some(ref vi) = self.vector_index {
                // Use empty embedding as placeholder — real implementation would
                // embed the query text first.
                let remaining = limit.saturating_sub(scored.len());
                if let Ok(cids) = vi.search(&[], remaining).await {
                    for cid in cids {
                        if seen_cids.contains(&cid.0) {
                            continue;
                        }
                        if let Ok(Some(m)) = self.store.get(&cid) {
                            seen_cids.insert(cid.0.clone());
                            // Semantic results get a baseline score below keyword matches
                            scored.push((0.01, m));
                        }
                    }
                }
            }
        }

        // ── Sort by score descending, truncate ──
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored
            .into_iter()
            .map(|(_, m)| metadata_to_search_result(m))
            .collect())
    }

    async fn lookup(&self, id: &str) -> Result<Vec<serde_json::Value>> {
        let cid = DatasetCid(id.to_string());
        match self.store.get(&cid)? {
            Some(m) => Ok(vec![serde_json::to_value(&m)?]),
            None => Ok(vec![]),
        }
    }

    async fn download(&self, id: &str) -> Result<Vec<serde_json::Value>> {
        let cid = DatasetCid(id.to_string());
        let _ = self.store.increment_hit(&cid, "download");
        let file_path = self
            .store
            .get_file_path(&cid)?
            .ok_or_else(|| anyhow::anyhow!("no local file for CID {id}"))?;
        Ok(vec![serde_json::json!({
            "cid": id,
            "file_path": file_path.to_string_lossy(),
            "info_hash": self.store.get(&cid)?.and_then(|m| m.info_hash),
        })])
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
    async fn search_ranks_by_relevance() {
        let dir = temp_dir("search-rank");
        let store = MetadataStore::open(&dir).unwrap();
        // c1: matches both title and tags
        store
            .put(&make_metadata(
                "c1",
                "finance stock market",
                &["finance", "stock"],
            ))
            .unwrap();
        // c2: matches only tags
        store
            .put(&make_metadata("c2", "some data", &["finance"]))
            .unwrap();

        let adapter = GuixuP2PAdapter::new(store);
        let results = adapter.search("finance stock", 10).await.unwrap();
        assert_eq!(results.len(), 2);
        // c1 should rank higher (matches title + tags)
        assert_eq!(results[0].cid.0, "c1");
    }

    #[tokio::test]
    async fn popularity_boosts_ranking() {
        let dir = temp_dir("search-pop");
        let store = MetadataStore::open(&dir).unwrap();
        store
            .put(&make_metadata("c1", "test dataset", &["test"]))
            .unwrap();
        store
            .put(&make_metadata("c2", "test dataset", &["test"]))
            .unwrap();
        // Boost c2 popularity
        for _ in 0..10 {
            store
                .increment_hit(&DatasetCid("c2".into()), "download")
                .unwrap();
        }

        let adapter = GuixuP2PAdapter::new(store);
        let results = adapter.search("test", 10).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].cid.0, "c2");
    }

    #[tokio::test]
    async fn search_increments_hit_count() {
        let dir = temp_dir("search-hits");
        let store = MetadataStore::open(&dir).unwrap();
        store
            .put(&make_metadata("c1", "test data", &["test"]))
            .unwrap();

        let adapter = GuixuP2PAdapter::new(store.clone());
        adapter.search("test", 10).await.unwrap();
        assert_eq!(
            store
                .get_hit_count(&DatasetCid("c1".into()), "search")
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn download_increments_hit_count() {
        let dir = temp_dir("download-hits");
        let store = MetadataStore::open(&dir).unwrap();
        store.put(&make_metadata("c1", "data1", &[])).unwrap();
        let file = dir.join("data.csv");
        std::fs::write(&file, "a,b\n1,2\n").unwrap();
        store
            .put_file_path(&DatasetCid("c1".into()), &file)
            .unwrap();

        let adapter = GuixuP2PAdapter::new(store.clone());
        adapter.download("c1").await.unwrap();
        assert_eq!(
            store
                .get_hit_count(&DatasetCid("c1".into()), "download")
                .unwrap(),
            1
        );
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
    async fn download_returns_file_path() {
        let dir = temp_dir("download-ok");
        let store = MetadataStore::open(&dir).unwrap();
        store.put(&make_metadata("c1", "data1", &[])).unwrap();
        let file = dir.join("data.csv");
        std::fs::write(&file, "a,b\n1,2\n").unwrap();
        store
            .put_file_path(&DatasetCid("c1".into()), &file)
            .unwrap();

        let adapter = GuixuP2PAdapter::new(store);
        let results = adapter.download("c1").await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0]["file_path"]
            .as_str()
            .unwrap()
            .contains("data.csv"));
    }

    #[tokio::test]
    async fn download_missing_returns_error() {
        let dir = temp_dir("download-miss");
        let store = MetadataStore::open(&dir).unwrap();
        let adapter = GuixuP2PAdapter::new(store);
        assert!(adapter.download("nonexistent").await.is_err());
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
