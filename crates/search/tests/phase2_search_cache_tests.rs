// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

//! Tests for Phase 2: Search Cache
//!
//! These tests verify:
//! - Cache key generation with SHA256 hashing
//! - TTL calculation per skill
//! - Cache get/set operations
//! - L1/L2 cache behavior

use chrono::Utc;
use data_core::types::{DataSource, DataType, DatasetCid, Did, License, Price, SearchResult};
use data_search::search_cache::SearchCache;

fn make_test_search_result(id: &str) -> SearchResult {
    SearchResult {
        cid: DatasetCid(format!("cid-{id}")),
        title: format!("Test Dataset {id}"),
        description: Some("A test dataset".into()),
        tags: vec!["test".to_string()],
        schema: data_core::types::DatasetSchema {
            columns: vec![],
            row_count: 100,
            size_bytes: 1024,
        },
        quality: None,
        price: Price::free(),
        license: License {
            spdx_id: "MIT".into(),
            commercial_use: true,
            derivative_allowed: true,
        },
        provider: Did("did:key:test".into()),
        source: DataSource::Kaggle,
        market: None,
        data_type: DataType::Tabular,
        created_at: Utc::now(),
        seller_endpoint: None,
        source_attributes: None,
        governance: None,
        provider_meta: None,
    }
}

#[test]
fn test_cache_key_generation_is_deterministic() {
    let key1 = SearchCache::cache_key("guixu_market", "machine learning", 20);
    let key2 = SearchCache::cache_key("guixu_market", "machine learning", 20);

    assert_eq!(key1, key2);
}

#[test]
fn test_cache_key_different_skills_produce_different_keys() {
    let key1 = SearchCache::cache_key("guixu_market", "machine learning", 20);
    let key2 = SearchCache::cache_key("huggingface", "machine learning", 20);

    assert_ne!(key1, key2);
}

#[test]
fn test_cache_key_different_queries_produce_different_keys() {
    let key1 = SearchCache::cache_key("guixu_market", "machine learning", 20);
    let key2 = SearchCache::cache_key("guixu_market", "deep learning", 20);

    assert_ne!(key1, key2);
}

#[test]
fn test_cache_key_different_limits_produce_different_keys() {
    let key1 = SearchCache::cache_key("guixu_market", "machine learning", 20);
    let key2 = SearchCache::cache_key("guixu_market", "machine learning", 50);

    assert_ne!(key1, key2);
}

#[test]
fn test_cache_key_normalizes_query_case() {
    let key1 = SearchCache::cache_key("guixu_market", "Machine Learning", 20);
    let key2 = SearchCache::cache_key("guixu_market", "machine learning", 20);

    assert_eq!(key1, key2);
}

#[test]
fn test_cache_key_trims_whitespace() {
    let key1 = SearchCache::cache_key("guixu_market", "  machine learning  ", 20);
    let key2 = SearchCache::cache_key("guixu_market", "machine learning", 20);

    assert_eq!(key1, key2);
}

#[test]
fn test_cache_key_format() {
    let key = SearchCache::cache_key("guixu_market", "test query", 20);

    // Key should be in format: search:v1:{skill_id}:{hash}:{limit}
    assert!(key.starts_with("search:v1:guixu_market:"));
    assert!(key.ends_with(":20"));
}

#[test]
fn test_ttl_for_guixu_market_skill() {
    let ttl = SearchCache::ttl_for_skill("guixu_market");

    // guixu_market should have 120s TTL as per spec
    assert_eq!(ttl, 120);
}

#[test]
fn test_ttl_for_other_skills() {
    let ttl = SearchCache::ttl_for_skill("huggingface");

    // Other skills should have default 300s TTL
    assert_eq!(ttl, 300);
}

#[test]
fn test_ttl_for_arxiv_skill() {
    let ttl = SearchCache::ttl_for_skill("arxiv");

    assert_eq!(ttl, 300);
}

#[tokio::test]
async fn test_cache_get_returns_none_for_miss() {
    let cache = SearchCache::new();
    let result = cache.get("nonexistent-key").await;

    assert!(result.is_none());
}

#[tokio::test]
async fn test_cache_set_and_get() {
    let cache = SearchCache::new();
    let results = vec![make_test_search_result("1"), make_test_search_result("2")];

    cache.set("test-key".to_string(), results.clone(), 60).await;

    let cached = cache.get("test-key").await;
    assert!(cached.is_some());
    let cached_results = cached.unwrap();
    assert_eq!(cached_results.len(), 2);
    assert_eq!(cached_results[0].cid, DatasetCid("cid-1".into()));
    assert_eq!(cached_results[1].cid, DatasetCid("cid-2".into()));
}

#[tokio::test]
async fn test_cache_overwrites_existing_value() {
    let cache = SearchCache::new();

    let results1 = vec![make_test_search_result("old")];
    let results2 = vec![make_test_search_result("new")];

    cache.set("overwrite-key".to_string(), results1, 60).await;
    cache.set("overwrite-key".to_string(), results2, 60).await;

    let cached = cache.get("overwrite-key").await;
    assert!(cached.is_some());
    assert_eq!(cached.unwrap()[0].cid, DatasetCid("cid-new".into()));
}

#[tokio::test]
async fn test_cache_with_different_limit_values() {
    let cache = SearchCache::new();
    let key10 = SearchCache::cache_key("test", "query", 10);
    let key20 = SearchCache::cache_key("test", "query", 20);

    cache
        .set(key10.clone(), vec![make_test_search_result("10")], 60)
        .await;
    cache
        .set(key20.clone(), vec![make_test_search_result("20")], 60)
        .await;

    let cached10 = cache.get(&key10).await;
    let cached20 = cache.get(&key20).await;

    assert!(cached10.is_some());
    assert!(cached20.is_some());
    assert_eq!(cached10.unwrap()[0].cid, DatasetCid("cid-10".into()));
    assert_eq!(cached20.unwrap()[0].cid, DatasetCid("cid-20".into()));
}

#[test]
fn test_cache_key_with_empty_query() {
    let key = SearchCache::cache_key("guixu_market", "", 20);

    // Empty query should still produce a valid key
    assert!(key.starts_with("search:v1:guixu_market:"));
    assert!(key.ends_with(":20"));
}

#[test]
fn test_cache_key_with_unicode_query() {
    let key = SearchCache::cache_key("guixu_market", "機械学習", 20);

    // Unicode query should still produce a valid key
    assert!(key.starts_with("search:v1:guixu_market:"));
}

#[test]
fn test_cache_key_with_long_query() {
    let long_query = "a".repeat(1000);
    let key = SearchCache::cache_key("guixu_market", &long_query, 20);

    // SHA256 hash should still produce reasonable length key
    assert!(key.starts_with("search:v1:guixu_market:"));
}
