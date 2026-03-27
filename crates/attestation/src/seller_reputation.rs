use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use tracing::debug;

use crate::client::{BaseChainClient, BasescanTx, PaymentToken};

/// Aggregated on-chain reputation for a seller address.
#[derive(Debug, Clone, Serialize)]
pub struct SellerReputation {
    pub address: String,
    /// Total number of successful sales.
    pub total_sales: u64,
    /// Volume breakdown by token (ETH, USDC, USDT).
    pub volume_by_token: HashMap<PaymentToken, f64>,
    /// Total volume normalized to USD-equivalent (ETH excluded, stablecoins at 1:1).
    pub total_volume_usd: f64,
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
    /// < 5 sales or < $10 volume.
    Newcomer,
    /// 5-20 sales and >= $10.
    Established,
    /// > 20 sales and >= $100 and >= 5 unique buyers.
    Trusted,
}

/// Fetch and compute seller reputation from Base chain transaction history.
///
/// Combines ETH native transfers and ERC-20 token transfers (USDC, USDT)
/// to build a comprehensive reputation profile.
pub async fn fetch_seller_reputation(
    client: &BaseChainClient,
    seller_address: &str,
) -> Result<SellerReputation> {
    let txs = client.get_contract_transactions(seller_address).await?;

    let sales: Vec<&BasescanTx> = txs
        .iter()
        .filter(|tx| {
            tx.is_error == "0"
                && tx
                    .function_name
                    .as_deref()
                    .unwrap_or("")
                    .contains("purchase")
        })
        .collect();

    let mut volume_by_token: HashMap<PaymentToken, f64> = HashMap::new();
    let mut buyers = std::collections::HashSet::new();
    let mut first_ts: u64 = u64::MAX;
    let mut latest_ts: u64 = 0;

    // Count ETH payments from normal transactions
    for tx in &sales {
        let wei = tx.value_wei.parse::<u128>().unwrap_or(0);
        if wei > 0 {
            *volume_by_token.entry(PaymentToken::ETH).or_default() +=
                PaymentToken::ETH.to_display_amount(wei);
        }
        buyers.insert(tx.from.to_lowercase());
        if let Ok(ts) = tx.timestamp.parse::<u64>() {
            first_ts = first_ts.min(ts);
            latest_ts = latest_ts.max(ts);
        }
    }

    // Count ERC-20 token payments (USDC/USDT)
    let token_txs = client.get_contract_token_transfers(seller_address).await?;
    for ttx in &token_txs {
        if let Some(token) = client.config.identify_token(&ttx.contract_address) {
            let raw = ttx.value.parse::<u128>().unwrap_or(0);
            *volume_by_token.entry(token).or_default() += token.to_display_amount(raw);
            buyers.insert(ttx.from.to_lowercase());
            if let Ok(ts) = ttx.timestamp.parse::<u64>() {
                first_ts = first_ts.min(ts);
                latest_ts = latest_ts.max(ts);
            }
        }
    }

    if first_ts == u64::MAX {
        first_ts = 0;
    }

    let total_sales = sales.len() as u64 + token_txs.len() as u64;

    // Stablecoins count as 1:1 USD; ETH excluded from USD total
    // (no price oracle — conservative approach)
    let total_volume_usd = volume_by_token
        .get(&PaymentToken::USDC)
        .copied()
        .unwrap_or(0.0)
        + volume_by_token
            .get(&PaymentToken::USDT)
            .copied()
            .unwrap_or(0.0);

    let tier = compute_tier(total_sales, total_volume_usd, buyers.len() as u64);

    debug!(
        address = seller_address,
        total_sales,
        total_volume_usd,
        unique_buyers = buyers.len(),
        ?tier,
        "seller reputation computed"
    );

    Ok(SellerReputation {
        address: seller_address.to_lowercase(),
        total_sales,
        volume_by_token,
        total_volume_usd,
        unique_buyers: buyers.len() as u64,
        first_sale_ts: first_ts,
        latest_sale_ts: latest_ts,
        tier,
    })
}

/// Compute reputation tier based on sales count, USD volume, and unique buyers.
pub(crate) fn compute_tier(sales: u64, volume_usd: f64, unique_buyers: u64) -> ReputationTier {
    if sales == 0 {
        ReputationTier::Unknown
    } else if sales > 20 && volume_usd >= 100.0 && unique_buyers >= 5 {
        ReputationTier::Trusted
    } else if sales >= 5 && volume_usd >= 10.0 {
        ReputationTier::Established
    } else {
        ReputationTier::Newcomer
    }
}
