// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use data_core::types::SearchResult;
use moka::future::{Cache, CacheBuilder};
use sha2::{Digest, Sha256};
use std::time::Duration;

const DEFAULT_TTL_SECS: u64 = 300;
const GUIXU_MARKET_TTL_SECS: u64 = 120;
const L1_MAX_CAPACITY: u64 = 10_000;

pub struct SearchCache {
    l1: Cache<String, Vec<SearchResult>>,
    #[cfg(feature = "redis-cache")]
    l2: Option<RedisCache>,
}

#[cfg(feature = "redis-cache")]
pub struct RedisCache {
    conn: redis::aio::ConnectionManager,
}

#[cfg(feature = "redis-cache")]
impl RedisCache {
    pub async fn new(redis_url: &str) -> anyhow::Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let conn = redis::aio::ConnectionManager::new(client).await?;
        Ok(Self { conn })
    }
}

impl SearchCache {
    pub fn new() -> Self {
        let l1 = CacheBuilder::new(L1_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(DEFAULT_TTL_SECS))
            .time_to_idle(Duration::from_secs(120))
            .build();
        Self {
            l1,
            #[cfg(feature = "redis-cache")]
            l2: None,
        }
    }

    #[cfg(feature = "redis-cache")]
    pub fn with_redis(redis_url: &str) -> anyhow::Result<Self> {
        let l1 = CacheBuilder::new(L1_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(DEFAULT_TTL_SECS))
            .time_to_idle(Duration::from_secs(120))
            .build();
        let l2 = Some(RedisCache::new(redis_url)?);
        Ok(Self { l1, l2 })
    }

    pub fn cache_key(skill_id: &str, query: &str, limit: usize) -> String {
        let normalized = query.trim().to_lowercase();
        let mut hasher = Sha256::new();
        hasher.update(normalized.as_bytes());
        let hash = hex::encode(hasher.finalize());
        format!("search:v1:{}:{}:{}", skill_id, hash, limit)
    }

    pub async fn get(&self, key: &str) -> Option<Vec<SearchResult>> {
        if let Some(result) = self.l1.get(key).await {
            return Some(result);
        }
        #[cfg(feature = "redis-cache")]
        if let Some(ref l2) = self.l2 {
            if let Some(result) = l2.get(key).await.ok().flatten() {
                self.l1.insert(key.to_string(), result.clone()).await;
                return Some(result);
            }
        }
        None
    }

    pub async fn set(&self, key: String, value: Vec<SearchResult>, ttl_secs: u64) {
        self.l1.insert(key.clone(), value.clone()).await;
        #[cfg(feature = "redis-cache")]
        if let Some(ref l2) = self.l2 {
            let _ = l2.set(&key, &value, ttl_secs).await;
        }
        #[cfg(not(feature = "redis-cache"))]
        let _ = ttl_secs;
    }

    pub fn ttl_for_skill(skill_id: &str) -> u64 {
        if skill_id == "guixu_market" {
            GUIXU_MARKET_TTL_SECS
        } else {
            DEFAULT_TTL_SECS
        }
    }
}

impl Default for SearchCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "redis-cache")]
impl RedisCache {
    pub async fn get(&self, key: &str) -> anyhow::Result<Option<Vec<SearchResult>>> {
        let mut conn = self.conn.clone();
        let data: Option<Vec<u8>> = conn.get(key).await?;
        Ok(data.and_then(|d| serde_json::from_slice(&d).ok()))
    }

    pub async fn set(
        &self,
        key: &str,
        value: &[SearchResult],
        ttl_secs: u64,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn.clone();
        let data = serde_json::to_vec(value)?;
        conn.set_ex(key, data, ttl_secs).await?;
        Ok(())
    }
}
