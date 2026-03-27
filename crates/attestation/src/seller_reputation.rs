use anyhow::Result;
use serde::Serialize;
use tracing::debug;

use crate::client::{BaseChainClient, BasescanTx};

/// Aggregated on-chain reputation for a seller address.
#[derive(Debug, Clone, Serialize)]
pub struct SellerReputation {
    pub address: String,
    /// Total number of successful sales (purchase txs where seller received ETH).
    pub total_sales: u64,
    /// Total ETH volume received from sales (in ETH, not wei).
    pub total_volume_eth: f64,
    /// Number of unique buyer addresses.
    pub unique_buyers: u64,
    /// Timestamp of first sale (unix seconds), 0 if none.
    pub first_sale_ts: u64,
    /// Timestamp of most recent sale (unix seconds), 0 if none.
    pub latest_sale_ts: u64,
    /// Trust tier derived from on-chain activity.
    pub tier: ReputationTier,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum ReputationTier {
    /// No on-chain sales history.
    Unknown,
    /// < 5 sales or < 0.1 ETH volume.
    Newcomer,
    /// 5-20 sales and >= 0.1 ETH.
    Established,
    /// > 20 sales and >= 1 ETH and >= 5 unique buyers.
    Trusted,
}

/// Fetch and compute seller reputation from Base chain transaction history.
///
/// Looks at all transactions where `seller_address` is the *recipient* of
/// `purchase()` calls on the DataTrade contract.
pub async fn fetch_seller_reputation(
    client: &BaseChainClient,
    seller_address: &str,
) -> Result<SellerReputation> {
    let txs = client.get_contract_transactions(seller_address).await?;

    // Filter: successful purchase() calls where this address is the seller.
    // In DataTrade.sol, purchase() sends ETH to the seller via l.seller.call{value}
    // So we look for incoming ETH transfers from the contract to the seller,
    // but Basescan txlist shows the original caller's tx. We instead look at
    // txs calling the contract's purchase function.
    let sales: Vec<&BasescanTx> = txs
        .iter()
        .filter(|tx| {
            tx.is_error == "0"
                && tx.function_name.as_deref().unwrap_or("").contains("purchase")
        })
        .collect();

    let total_sales = sales.len() as u64;
    let total_volume_eth: f64 = sales
        .iter()
        .filter_map(|tx| tx.value_wei.parse::<u128>().ok())
        .map(|wei| wei as f64 / 1e18)
        .sum();

    let mut buyers = std::collections::HashSet::new();
    let mut first_ts: u64 = u64::MAX;
    let mut latest_ts: u64 = 0;

    for tx in &sales {
        buyers.insert(tx.from.to_lowercase());
        if let Ok(ts) = tx.timestamp.parse::<u64>() {
            first_ts = first_ts.min(ts);
            latest_ts = latest_ts.max(ts);
        }
    }

    if first_ts == u64::MAX {
        first_ts = 0;
    }

    let tier = compute_tier(total_sales, total_volume_eth, buyers.len() as u64);

    debug!(
        address = seller_address,
        total_sales,
        total_volume_eth,
        unique_buyers = buyers.len(),
        ?tier,
        "seller reputation computed"
    );

    Ok(SellerReputation {
        address: seller_address.to_lowercase(),
        total_sales,
        total_volume_eth,
        unique_buyers: buyers.len() as u64,
        first_sale_ts: first_ts,
        latest_sale_ts: latest_ts,
        tier,
    })
}

pub(crate) fn compute_tier(sales: u64, volume: f64, unique_buyers: u64) -> ReputationTier {
    if sales == 0 {
        ReputationTier::Unknown
    } else if sales > 20 && volume >= 1.0 && unique_buyers >= 5 {
        ReputationTier::Trusted
    } else if sales >= 5 && volume >= 0.1 {
        ReputationTier::Established
    } else {
        ReputationTier::Newcomer
    }
}
