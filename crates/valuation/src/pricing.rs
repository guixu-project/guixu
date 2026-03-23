use anyhow::Result;
use data_core::metadata::DatasetMetadata;

/// Dynamic pricing engine for paid datasets.
pub struct PricingEngine;

impl PricingEngine {
    /// Compute dynamic price based on market factors.
    pub fn compute_price(&self, metadata: &DatasetMetadata, market: &MarketData) -> f64 {
        let base = metadata.price.amount;
        if base == 0.0 {
            return 0.0;
        }

        let freshness_decay = {
            let age = (chrono::Utc::now() - metadata.updated_at).num_days() as f64;
            (1.0 - age * 0.005).max(0.3) // min 30% of base
        };

        let demand = (1.0 + market.download_count as f64 * 0.001).min(2.0);
        let scarcity = if market.alternative_count == 0 {
            1.5
        } else {
            1.0 / (1.0 + market.alternative_count as f64 * 0.1)
        };
        let reputation = 0.5 + market.provider_reputation * 0.5;

        base * freshness_decay * demand * scarcity * reputation
    }
}

/// Market data fetched from network / oracle.
#[derive(Debug, Clone, Default)]
pub struct MarketData {
    pub download_count: u64,
    pub alternative_count: u64,
    pub provider_reputation: f64, // 0.0 - 1.0
    pub avg_market_price: f64,
}
