use anyhow::{Context, Result};
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::encrypt_zip;

const DEFAULT_MARKET_URL: &str = "https://guixu.org";

#[derive(Debug, Serialize)]
pub struct ListingRequest {
    pub title: String,
    pub description: String,
    /// Price in ETH as string, e.g. "0.01"
    pub price_eth: String,
    /// Password used to encrypt the zip (will be hashed for upload)
    pub password: String,
    /// Seller wallet address (hex)
    pub seller_address: String,
    /// On-chain contract address
    pub contract_address: String,
}

#[derive(Debug, Deserialize)]
pub struct ListingResponse {
    pub id: String,
}

/// Encrypt a local file as zip, then upload to guixu.org market.
///
/// Steps:
/// 1. AES-256 encrypt + compress file into zip
/// 2. POST multipart form to `/api/upload`
///
/// Note: on-chain listing (smart-contract `list` + `depositPassword`) must be
/// done separately by the caller via an Ethereum wallet/signer before calling
/// this function. The `listing_id` returned from the contract call should be
/// passed here.
pub async fn publish_to_market(
    path: &Path,
    listing_id: &str,
    req: &ListingRequest,
    market_url: Option<&str>,
) -> Result<ListingResponse> {
    let base = market_url.unwrap_or(DEFAULT_MARKET_URL);

    // 1. Encrypt zip
    let zip_bytes = encrypt_zip(path, &req.password)?;

    // 2. Compute password keccak256 hash (same as ethers.keccak256 on UTF-8)
    let password_hash = keccak256_hex(&req.password);

    // 3. Convert price to wei
    let price_wei = eth_to_wei(&req.price_eth)?;

    // 4. Build multipart form matching /api/upload expectations
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("data.zip");

    let form = multipart::Form::new()
        .text("listingId", listing_id.to_string())
        .text("title", req.title.clone())
        .text("description", req.description.clone())
        .text("priceWei", price_wei)
        .text("sellerAddress", req.seller_address.clone())
        .text("contractAddress", req.contract_address.clone())
        .text("passwordHash", password_hash)
        .part(
            "file",
            multipart::Part::bytes(zip_bytes)
                .file_name(format!("{file_name}.zip"))
                .mime_str("application/zip")?,
        );

    // 5. Upload
    let resp = reqwest::Client::new()
        .post(format!("{base}/api/upload"))
        .multipart(form)
        .send()
        .await
        .context("upload to market")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("upload failed ({status}): {body}");
    }

    resp.json::<ListingResponse>()
        .await
        .context("parse upload response")
}

/// Keccak-256 hash, returned as 0x-prefixed hex (matches ethers.keccak256).
fn keccak256_hex(input: &str) -> String {
    use sha3::{Digest as _, Keccak256};
    let hash = Keccak256::digest(input.as_bytes());
    format!("0x{}", hex::encode(hash))
}

/// Convert ETH decimal string to wei string.
fn eth_to_wei(eth: &str) -> Result<String> {
    let parts: Vec<&str> = eth.split('.').collect();
    let (whole, frac) = match parts.len() {
        1 => (parts[0], ""),
        2 => (parts[0], parts[1]),
        _ => anyhow::bail!("invalid ETH amount: {eth}"),
    };
    // Pad fractional part to 18 decimals
    let frac_padded = format!("{:0<18}", frac);
    if frac_padded.len() > 18 {
        anyhow::bail!("too many decimal places in: {eth}");
    }
    let wei_str = format!("{}{}", whole, frac_padded);
    // Strip leading zeros but keep at least "0"
    let trimmed = wei_str.trim_start_matches('0');
    Ok(if trimmed.is_empty() { "0" } else { trimmed }.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eth_to_wei_whole_number() {
        assert_eq!(eth_to_wei("1").unwrap(), "1000000000000000000");
    }

    #[test]
    fn eth_to_wei_fractional() {
        assert_eq!(eth_to_wei("0.01").unwrap(), "10000000000000000");
    }

    #[test]
    fn eth_to_wei_zero() {
        assert_eq!(eth_to_wei("0").unwrap(), "0");
    }

    #[test]
    fn eth_to_wei_small_amount() {
        assert_eq!(eth_to_wei("0.001").unwrap(), "1000000000000000");
    }

    #[test]
    fn eth_to_wei_invalid_format() {
        assert!(eth_to_wei("1.2.3").is_err());
    }

    #[test]
    fn keccak256_matches_ethers() {
        // ethers.keccak256(ethers.toUtf8Bytes("hello")) =
        // 0x1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8
        let hash = keccak256_hex("hello");
        assert_eq!(
            hash,
            "0x1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8"
        );
    }

    #[test]
    fn listing_request_serializes() {
        let req = ListingRequest {
            title: "Test".into(),
            description: "Desc".into(),
            price_eth: "0.01".into(),
            password: "pw".into(),
            seller_address: "0xabc".into(),
            contract_address: "0xdef".into(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["title"], "Test");
        assert_eq!(json["price_eth"], "0.01");
    }
}
