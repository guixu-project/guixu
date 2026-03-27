use anyhow::Result;
use serde::Serialize;
use tracing::debug;

use crate::client::{BaseChainClient, BasescanTx, PaymentAmount, PaymentToken};

/// A single buyer review extracted from on-chain transaction data.
#[derive(Debug, Clone, Serialize)]
pub struct BuyerReview {
    pub tx_hash: String,
    pub buyer: String,
    pub listing_id: String,
    pub rating: u8,
    pub comment: String,
    pub timestamp: u64,
    pub payment: PaymentAmount,
}

/// Aggregated review summary for a dataset listing.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewSummary {
    pub listing_id: String,
    pub total_reviews: u64,
    pub avg_rating: f64,
    pub reviews: Vec<BuyerReview>,
}

/// Fetch buyer reviews for a specific listing from Base chain.
///
/// Reviews are encoded in the transaction input data as extra bytes appended
/// after the standard `purchase(string)` ABI-encoded calldata.
///
/// Encoding convention (appended after ABI calldata):
///   [1 byte: rating 1-5] [remaining: UTF-8 comment]
///
/// Transactions without the extra bytes are purchases with no review.
///
/// Supports both ETH native payments and ERC-20 token payments (USDC/USDT).
pub async fn fetch_buyer_reviews(
    client: &BaseChainClient,
    listing_id: &str,
) -> Result<Vec<BuyerReview>> {
    let txs = client
        .get_transactions(&client.config.contract_address, 0, 99999999)
        .await?;

    let listing_id_hex = hex::encode(listing_id);
    let mut reviews = Vec::new();

    // Build a map of tx_hash -> token transfers for ERC-20 payment detection
    let token_txs = client
        .get_contract_token_transfers(&client.config.contract_address)
        .await?;
    let token_payments: std::collections::HashMap<String, PaymentAmount> = token_txs
        .iter()
        .filter_map(|ttx| {
            let token = client.config.identify_token(&ttx.contract_address)?;
            let raw = ttx.value.parse::<u128>().ok()?;
            Some((
                ttx.hash.to_lowercase(),
                PaymentAmount { token, amount: token.to_display_amount(raw) },
            ))
        })
        .collect();

    for tx in &txs {
        if tx.is_error != "0" {
            continue;
        }
        let fname = tx.function_name.as_deref().unwrap_or("");
        if !fname.contains("purchase") {
            continue;
        }
        if !tx.input.contains(&listing_id_hex) {
            continue;
        }
        if let Some(review) = parse_review_from_input(&tx.input, listing_id, tx, &token_payments) {
            reviews.push(review);
        }
    }

    debug!(listing_id, count = reviews.len(), "buyer reviews fetched");
    Ok(reviews)
}

/// Summarize reviews for a listing.
pub fn summarize_reviews(listing_id: &str, reviews: &[BuyerReview]) -> ReviewSummary {
    let avg_rating = if reviews.is_empty() {
        0.0
    } else {
        reviews.iter().map(|r| r.rating as f64).sum::<f64>() / reviews.len() as f64
    };
    ReviewSummary {
        listing_id: listing_id.to_string(),
        total_reviews: reviews.len() as u64,
        avg_rating,
        reviews: reviews.to_vec(),
    }
}

/// Parse review data from transaction input.
///
/// Determines payment token: if the tx has a matching ERC-20 token transfer,
/// uses that; otherwise falls back to native ETH value.
pub(crate) fn parse_review_from_input(
    input_hex: &str,
    listing_id: &str,
    tx: &BasescanTx,
    token_payments: &std::collections::HashMap<String, PaymentAmount>,
) -> Option<BuyerReview> {
    let input = input_hex.strip_prefix("0x").unwrap_or(input_hex);
    let bytes = hex::decode(input).ok()?;

    if bytes.len() < 100 {
        return None;
    }

    let str_len_offset = 4 + 32;
    if bytes.len() < str_len_offset + 32 {
        return None;
    }
    let str_len = u256_to_usize(&bytes[str_len_offset..str_len_offset + 32])?;
    let padded_str_len = (str_len + 31) / 32 * 32;
    let abi_end = 4 + 32 + 32 + padded_str_len;

    if bytes.len() <= abi_end {
        return None;
    }

    let review_bytes = &bytes[abi_end..];
    if review_bytes.is_empty() {
        return None;
    }

    let rating = review_bytes[0].clamp(1, 5);
    let comment = if review_bytes.len() > 1 {
        String::from_utf8_lossy(&review_bytes[1..]).trim().to_string()
    } else {
        String::new()
    };

    let ts = tx.timestamp.parse::<u64>().unwrap_or(0);

    // Determine payment: check for ERC-20 token transfer first, fallback to ETH
    let payment = token_payments
        .get(&tx.hash.to_lowercase())
        .cloned()
        .unwrap_or_else(|| {
            let wei = tx.value_wei.parse::<u128>().unwrap_or(0);
            PaymentAmount {
                token: PaymentToken::ETH,
                amount: PaymentToken::ETH.to_display_amount(wei),
            }
        });

    Some(BuyerReview {
        tx_hash: tx.hash.clone(),
        buyer: tx.from.to_lowercase(),
        listing_id: listing_id.to_string(),
        rating,
        comment,
        timestamp: ts,
        payment,
    })
}

fn u256_to_usize(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 32 {
        return None;
    }
    let val = u64::from_be_bytes(bytes[24..32].try_into().ok()?);
    Some(val as usize)
}
