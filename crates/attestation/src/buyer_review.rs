use anyhow::Result;
use serde::Serialize;
use tracing::debug;

use crate::client::{BaseChainClient, BasescanTx};

/// A single buyer review extracted from on-chain transaction data.
#[derive(Debug, Clone, Serialize)]
pub struct BuyerReview {
    pub tx_hash: String,
    pub buyer: String,
    pub listing_id: String,
    pub rating: u8,
    pub comment: String,
    pub timestamp: u64,
    pub value_eth: f64,
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
pub async fn fetch_buyer_reviews(
    client: &BaseChainClient,
    listing_id: &str,
) -> Result<Vec<BuyerReview>> {
    // Get all txs to the contract
    // We scan broadly since we don't know buyer addresses upfront
    let txs = client
        .get_transactions(&client.config.contract_address, 0, 99999999)
        .await?;

    let listing_id_hex = hex::encode(listing_id);
    let mut reviews = Vec::new();

    for tx in &txs {
        if tx.is_error != "0" {
            continue;
        }
        // Must be a purchase() call
        let fname = tx.function_name.as_deref().unwrap_or("");
        if !fname.contains("purchase") {
            continue;
        }
        // Check if this tx is for our listing by looking for listing_id in input
        if !tx.input.contains(&listing_id_hex) {
            continue;
        }
        // Try to extract review from trailing bytes
        if let Some(review) = parse_review_from_input(&tx.input, listing_id, tx) {
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
/// ABI encoding of `purchase(string listingId)`:
///   4 bytes selector + 32 bytes offset + 32 bytes length + padded string data
///
/// Our convention: any bytes after the ABI-encoded calldata boundary are the
/// review payload: `[u8 rating][utf8 comment...]`
///
/// If the input exactly matches the standard ABI encoding length, there's no
/// review — it's a plain purchase.
pub(crate) fn parse_review_from_input(
    input_hex: &str,
    listing_id: &str,
    tx: &BasescanTx,
) -> Option<BuyerReview> {
    let input = input_hex.strip_prefix("0x").unwrap_or(input_hex);
    let bytes = hex::decode(input).ok()?;

    // Minimum ABI-encoded purchase(string): 4 + 32 + 32 + 32 = 100 bytes
    // (selector + offset + length + one 32-byte padded chunk)
    if bytes.len() < 100 {
        return None;
    }

    // Calculate expected ABI length: 4 + 32 (offset) + 32 (length) + ceil32(string_len)
    let str_len_offset = 4 + 32; // offset to the length field
    if bytes.len() < str_len_offset + 32 {
        return None;
    }
    let str_len = u256_to_usize(&bytes[str_len_offset..str_len_offset + 32])?;
    let padded_str_len = (str_len + 31) / 32 * 32; // ceil to 32
    let abi_end = 4 + 32 + 32 + padded_str_len;

    // Extra bytes after ABI = review payload
    if bytes.len() <= abi_end {
        return None; // no review attached
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
    let value_eth = tx.value_wei.parse::<u128>().unwrap_or(0) as f64 / 1e18;

    Some(BuyerReview {
        tx_hash: tx.hash.clone(),
        buyer: tx.from.to_lowercase(),
        listing_id: listing_id.to_string(),
        rating,
        comment,
        timestamp: ts,
        value_eth,
    })
}

fn u256_to_usize(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 32 {
        return None;
    }
    // Only care about last 8 bytes (usize range)
    let val = u64::from_be_bytes(bytes[24..32].try_into().ok()?);
    Some(val as usize)
}
