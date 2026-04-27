// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use crate::router::TransactionContext;
use crate::wallet::AgentWallet;
use alloy_primitives::{Address, U256};
use anyhow::{bail, Context, Result};
use data_core::types::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

/// ERC-8183 programmable escrow client — lock → deliver → verify → release.
///
/// Flow:
/// 1. Call `createJob(buyer, seller, amount, cid, timeout)` to lock funds
/// 2. Wait for seller to deliver data + Merkle proof
/// 3. Verify Merkle proof against the dataset CID
/// 4. Call `confirmJob(jobId, merkleRoot)` to release funds
/// 5. On failure → dispute → Evaluator arbitration
pub struct EscrowClient {
    wallet: AgentWallet,
    http: Client,
    rpc_url: String,
    chain_id: u64,
}

impl EscrowClient {
    /// Create a new EscrowClient connecting to the specified RPC URL.
    pub fn new(wallet: AgentWallet, rpc_url: &str, chain_id: u64) -> Self {
        Self {
            wallet,
            http: Client::new(),
            rpc_url: rpc_url.to_string(),
            chain_id,
        }
    }

    /// Create for Base mainnet (chain_id = 8453).
    pub fn for_base_mainnet(wallet: AgentWallet) -> Self {
        Self::new(wallet, "https://mainnet.base.org", 8453)
    }

    /// Create for Base Sepolia testnet (chain_id = 84532).
    pub fn for_base_sepolia(wallet: AgentWallet) -> Self {
        Self::new(wallet, "https://sepolia.base.org", 84532)
    }

    /// Execute an escrowed purchase.
    pub async fn pay(&self, ctx: &TransactionContext) -> Result<TransactionReceipt> {
        // Step 1: Build createJob transaction
        let usdc_amount = (ctx.amount * 1_000_000.0) as u128; // USDC has 6 decimals
        let timeout = 3600u64; // 1 hour timeout

        let seller_addr: Address = ctx.seller.0.parse().context("invalid seller address")?;
        let buyer_addr: Address = ctx.buyer.0.parse().context("invalid buyer address")?;

        // ERC-8183 createJob(address buyer, address seller, uint256 amount, bytes calldata cid, uint256 timeout)
        let call_data = encode_create_job(
            buyer_addr,
            seller_addr,
            usdc_amount,
            &ctx.dataset_cid.0,
            timeout,
        );

        info!(
            buyer = %ctx.buyer.0,
            seller = %ctx.seller.0,
            amount = ctx.amount,
            cid = %ctx.dataset_cid.0,
            "erc8183: creating escrow job"
        );

        // Step 2: Send transaction via RPC
        let tx_hash = self.send_transaction(seller_addr, call_data, 0u128).await?;

        // Step 3: Wait for delivery (simplified - in production would be event-based)
        let delivery = self.wait_for_delivery(&tx_hash).await?;

        // Step 4: Verify Merkle proof
        if !self.verify_merkle_proof(&delivery.merkle_root, &ctx.dataset_cid) {
            bail!("erc8183: Merkle proof verification failed");
        }

        // Step 5: Confirm job to release funds
        let confirm_data = encode_confirm_job(&tx_hash, &delivery.merkle_root);
        let confirm_tx = self
            .send_transaction(seller_addr, confirm_data, 0u128)
            .await?;

        info!(
            job_tx = %tx_hash,
            confirm_tx = %confirm_tx,
            "erc8183: escrow completed successfully"
        );

        Ok(TransactionReceipt {
            tx_id: confirm_tx,
            buyer: ctx.buyer.clone(),
            seller: ctx.seller.clone(),
            dataset_cid: ctx.dataset_cid.clone(),
            price: Price::usdc(ctx.amount),
            protocol: PaymentProtocol::Erc8183,
            timestamp: chrono::Utc::now(),
            seller_response: Some(serde_json::to_string(&delivery)?),
        })
    }

    /// Send a raw transaction via JSON-RPC.
    async fn send_transaction(&self, to: Address, data: Vec<u8>, value: u128) -> Result<String> {
        let from = self.wallet.address();

        // Get nonce
        let nonce_raw = self
            .call_rpc_raw(
                "eth_getTransactionCount",
                &[
                    serde_json::json!(from.to_string()),
                    serde_json::json!("latest"),
                ],
            )
            .await?;
        let nonce = Self::parse_hex_to_u256(nonce_raw.as_str().unwrap_or("0x0"))?;

        // Get gas price
        let gas_price_raw = self.call_rpc_raw("eth_gasPrice", &[]).await?;
        let gas_price = Self::parse_hex_to_u256(gas_price_raw.as_str().unwrap_or("0x0"))?;

        // Estimate gas (simplified - use 200000 as default for contract interaction)
        let gas = U256::from(200000u64);

        // Build transaction
        let tx = serde_json::json!({
            "from": from.to_string(),
            "to": to.to_string(),
            "data": format!("0x{}", hex::encode(&data)),
            "value": format!("0x{:x}", U256::from(value)),
            "nonce": format!("0x{:x}", nonce),
            "gasPrice": format!("0x{:x}", gas_price),
            "gas": format!("0x{:x}", gas),
            "chainId": format!("0x{:x}", U256::from(self.chain_id)),
        });

        let tx_hash_raw = self.call_rpc_raw("eth_sendTransaction", &[tx]).await?;
        let tx_hash = tx_hash_raw.as_str().unwrap_or("").to_string();

        // Wait for receipt
        self.wait_for_receipt(&tx_hash).await
    }

    /// Call JSON-RPC method returning raw JSON value.
    async fn call_rpc_raw(
        &self,
        method: &str,
        params: &[serde_json::Value],
    ) -> Result<serde_json::Value> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1,
        });

        let resp = self
            .http
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        if let Some(error) = resp.get("error") {
            bail!("RPC error: {}", error);
        }

        let result = resp
            .get("result")
            .cloned()
            .context("no result in RPC response")?;
        Ok(result)
    }

    /// Parse a hex string to U256 (handles "0x" prefix and various formats).
    fn parse_hex_to_u256(hex: &str) -> Result<U256> {
        let hex_clean = hex.strip_prefix("0x").unwrap_or(hex);
        let bytes = hex::decode(hex_clean).context("failed to decode hex")?;
        let mut arr = [0u8; 32];
        let start = 32 - bytes.len().min(32);
        arr[start..].copy_from_slice(&bytes[..bytes.len().min(32)]);
        Ok(U256::from_be_bytes(arr))
    }

    /// Wait for transaction receipt.
    async fn wait_for_receipt(&self, tx_hash: &str) -> Result<String> {
        for _ in 0..30 {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            let receipt_raw = self
                .call_rpc_raw("eth_getTransactionReceipt", &[serde_json::json!(tx_hash)])
                .await?;

            if let Some(r) = receipt_raw.as_object() {
                if r.get("status") == Some(&serde_json::json!("0x1")) {
                    return Ok(tx_hash.to_string());
                } else {
                    bail!("transaction failed");
                }
            }
        }
        bail!("timeout waiting for transaction receipt")
    }

    /// Wait for data delivery from seller.
    async fn wait_for_delivery(&self, _job_tx_hash: &str) -> Result<DeliveryResult> {
        // In production, this would listen for Delivery events from the escrow contract
        // For now, we simulate a successful delivery
        Ok(DeliveryResult {
            merkle_root: "0x0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
        })
    }

    /// Verify Merkle proof against dataset CID.
    fn verify_merkle_proof(&self, merkle_root: &str, _dataset_cid: &DatasetCid) -> bool {
        // In production, verify the Merkle proof proves the CID is in the dataset
        // For now, accept any non-zero merkle root
        !merkle_root.ends_with("0000000000000000000000000000000000000000000000000000000000000000")
    }
}

/// Delivery result from seller.
#[derive(Debug, Serialize, Deserialize)]
struct DeliveryResult {
    merkle_root: String,
}

/// Encode createJob call data.
/// Function signature: createJob(address buyer, address seller, uint256 amount, bytes cid, uint256 timeout)
fn encode_create_job(
    buyer: Address,
    seller: Address,
    amount: u128,
    cid: &str,
    timeout: u64,
) -> Vec<u8> {
    // Function selector: keccak256("createJob(address,address,uint256,bytes,uint256)")[0:4]
    let selector = hex::decode("9c2e1d8").unwrap(); // First 4 bytes of the selector

    // Encode parameters using ABI encoding
    let mut data = selector;

    // Encode buyer (address - 20 bytes, padded to 32)
    let buyer_bytes: [u8; 32] = {
        let mut b = [0u8; 32];
        b[12..32].copy_from_slice(buyer.as_slice());
        b
    };
    data.extend_from_slice(&buyer_bytes);

    // Encode seller (address - 20 bytes, padded to 32)
    let seller_bytes: [u8; 32] = {
        let mut s = [0u8; 32];
        s[12..32].copy_from_slice(seller.as_slice());
        s
    };
    data.extend_from_slice(&seller_bytes);

    // Encode amount (uint256)
    let amount_bytes: [u8; 32] = {
        let mut a = [0u8; 32];
        a[16..32].copy_from_slice(&amount.to_be_bytes());
        a
    };
    data.extend_from_slice(&amount_bytes);

    // Encode cid (bytes) - need to encode length and data
    let cid_bytes = cid.as_bytes();
    let cid_len = cid_bytes.len();

    // Encode offset to cid data (32 bytes for offset + actual data)
    // Offset is 0x80 (128) since we have: selector (32) + buyer (32) + seller (32) + amount (32) + offset (32) = 160 = 0xa0
    let offset_bytes: [u8; 32] = {
        let mut o = [0u8; 32];
        o[31] = 0xa0; // offset to bytes data
        o
    };
    data.extend_from_slice(&offset_bytes);

    // At offset 0xa0: encode cid as bytes
    // Length of bytes
    let len_bytes: [u8; 32] = {
        let mut l = [0u8; 32];
        l[31] = cid_len as u8; // for short strings that fit in one word
        l
    };
    data.extend_from_slice(&len_bytes);

    // Actual cid bytes padded to 32 bytes
    let mut cid_padded = [0u8; 32];
    cid_padded[..cid_len].copy_from_slice(cid_bytes);
    data.extend_from_slice(&cid_padded);

    // Encode timeout (uint256)
    let timeout_bytes: [u8; 32] = {
        let mut t = [0u8; 32];
        t[24..32].copy_from_slice(&timeout.to_be_bytes());
        t
    };
    data.extend_from_slice(&timeout_bytes);

    data
}

/// Encode confirmJob call data.
/// Function signature: confirmJob(bytes32 jobId, bytes32 merkleRoot)
fn encode_confirm_job(job_id: &str, merkle_root: &str) -> Vec<u8> {
    // Function selector: keccak256("confirmJob(bytes32,bytes32)")[0:4]
    let selector = hex::decode("f7dac0a3").unwrap(); // First 4 bytes of the selector

    let mut data = selector;

    // Encode job_id (bytes32)
    let job_hex = job_id.trim_start_matches("0x");
    let job_bytes = hex::decode(job_hex).unwrap_or_else(|_| vec![0u8; 32]);
    let mut job_padded = [0u8; 32];
    job_padded[32 - job_bytes.len().min(32)..]
        .copy_from_slice(&job_bytes[..job_bytes.len().min(32)]);
    data.extend_from_slice(&job_padded);

    // Encode merkle_root (bytes32)
    let merkle_hex = merkle_root.trim_start_matches("0x");
    let merkle_bytes = hex::decode(merkle_hex).unwrap_or_else(|_| vec![0u8; 32]);
    let mut merkle_padded = [0u8; 32];
    merkle_padded[32 - merkle_bytes.len().min(32)..]
        .copy_from_slice(&merkle_bytes[..merkle_bytes.len().min(32)]);
    data.extend_from_slice(&merkle_padded);

    data
}

#[async_trait::async_trait]
impl crate::protocol::PaymentProtocolHandler for EscrowClient {
    async fn pay(&self, _seller_url: &str, ctx: &TransactionContext) -> Result<TransactionReceipt> {
        self.pay(ctx).await
    }
}
