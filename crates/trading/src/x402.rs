use alloy_primitives::{Address, U256};
use anyhow::{bail, Context, Result};
use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use data_core::types::*;

use crate::router::TransactionContext;
use crate::wallet::AgentWallet;

/// USDC contract addresses per chain.
const USDC_BASE: &str = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
const USDC_BASE_SEPOLIA: &str = "0x036CbD53842c5426634e7929541eC2318f3dCF7e";

/// Coinbase-hosted facilitator for mainnet.
const CDP_FACILITATOR: &str = "https://x402.coinbase.com";

/// x402 protocol client — single-shot HTTP 402 micropayments (USDC on Base).
///
/// Flow:
/// 1. GET seller endpoint → receive 402 + PAYMENT-REQUIRED header
/// 2. Parse payment requirements (payTo, price, network, scheme)
/// 3. Sign EIP-712 TransferWithAuthorization for USDC
/// 4. Retry with PAYMENT-SIGNATURE header
/// 5. Receive 200 + PAYMENT-RESPONSE header with settlement receipt
pub struct X402Client {
    wallet: AgentWallet,
    http: Client,
    facilitator_url: String,
    testnet: bool,
}

/// Payment requirements returned in the 402 response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaymentRequired {
    accepts: Vec<AcceptedScheme>,
    #[allow(dead_code)]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcceptedScheme {
    scheme: String,
    network: String,
    #[serde(alias = "maxAmountRequired")]
    max_amount_required: Option<String>,
    #[serde(alias = "price")]
    price: Option<String>,
    #[serde(alias = "payTo")]
    pay_to: String,
    #[allow(dead_code)]
    extra: Option<serde_json::Value>,
}

/// Signed payment payload sent in the retry request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PaymentPayload {
    payload: PaymentAuthorization,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PaymentAuthorization {
    authorization: TransferAuth,
    signature: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferAuth {
    from: String,
    to: String,
    value: String,
    valid_after: String,
    valid_before: String,
    nonce: String,
}

/// Settlement response from the PAYMENT-RESPONSE header.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaymentResponse {
    #[allow(dead_code)]
    success: bool,
    #[serde(alias = "txHash")]
    tx_hash: Option<String>,
    #[allow(dead_code)]
    network: Option<String>,
}

impl X402Client {
    pub fn new(wallet: AgentWallet, testnet: bool) -> Self {
        Self {
            wallet,
            http: Client::new(),
            facilitator_url: CDP_FACILITATOR.to_string(),
            testnet,
        }
    }

    pub fn with_facilitator(mut self, url: String) -> Self {
        self.facilitator_url = url;
        self
    }

    /// Execute an x402 payment against a seller endpoint.
    pub async fn pay(
        &self,
        seller_url: &str,
        ctx: &TransactionContext,
    ) -> Result<TransactionReceipt> {
        // Step 1: Initial request → expect 402
        let resp = self.http.get(seller_url).send().await?;

        if resp.status().as_u16() != 402 {
            bail!(
                "x402: expected 402 from seller, got {}",
                resp.status().as_u16()
            );
        }

        // Step 2: Parse payment requirements
        let payment_header = resp
            .headers()
            .get("payment-required")
            .or_else(|| resp.headers().get("x-payment"))
            .context("x402: missing PAYMENT-REQUIRED header")?
            .to_str()?;

        let requirements: PaymentRequired = serde_json::from_str(payment_header)
            .or_else(|_| {
                let decoded = base64::engine::general_purpose::STANDARD.decode(payment_header)?;
                serde_json::from_slice::<PaymentRequired>(&decoded).map_err(anyhow::Error::from)
            })
            .context("x402: failed to parse payment requirements")?;

        let scheme = requirements
            .accepts
            .first()
            .context("x402: no accepted payment schemes")?;

        if scheme.scheme != "exact" {
            bail!("x402: unsupported scheme '{}', expected 'exact'", scheme.scheme);
        }

        let pay_to: Address = scheme.pay_to.parse().context("x402: invalid payTo address")?;
        let price_str = scheme
            .max_amount_required
            .as_deref()
            .or(scheme.price.as_deref())
            .context("x402: no price in payment requirements")?;

        // Parse amount — could be "$0.01" or raw units "10000"
        let amount_raw = if price_str.starts_with('$') {
            let dollars: f64 = price_str[1..].parse()?;
            (dollars * 1_000_000.0) as u128 // USDC has 6 decimals
        } else {
            price_str.parse::<u128>()?
        };
        let amount = U256::from(amount_raw);

        let chain_id = parse_chain_id(&scheme.network)?;
        let usdc_token: Address = if self.testnet {
            USDC_BASE_SEPOLIA.parse()?
        } else {
            USDC_BASE.parse()?
        };

        // Step 3: Sign EIP-712 TransferWithAuthorization
        let nonce = U256::from(rand_nonce());
        let deadline = U256::from(chrono::Utc::now().timestamp() as u64 + 3600);

        let hash =
            self.wallet
                .eip712_transfer_hash(pay_to, amount, nonce, deadline, usdc_token, chain_id);

        let signature = self.wallet.sign_hash(hash).await?;
        let sig_hex = format!("0x{}", hex::encode(&signature));

        info!(
            pay_to = %pay_to,
            amount = %amount,
            chain_id,
            "x402: signing payment"
        );

        // Step 4: Build signed payload and retry
        let payload = PaymentPayload {
            payload: PaymentAuthorization {
                authorization: TransferAuth {
                    from: format!("{}", self.wallet.address()),
                    to: format!("{pay_to}"),
                    value: amount.to_string(),
                    valid_after: "0".into(),
                    valid_before: deadline.to_string(),
                    nonce: nonce.to_string(),
                },
                signature: sig_hex,
            },
        };

        let encoded = base64::engine::general_purpose::STANDARD
            .encode(serde_json::to_string(&payload)?);

        let resp = self
            .http
            .get(seller_url)
            .header("payment-signature", &encoded)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("x402: payment rejected with {status}: {body}");
        }

        // Step 5: Parse settlement receipt
        let tx_hash = resp
            .headers()
            .get("payment-response")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| {
                serde_json::from_str::<PaymentResponse>(h)
                    .ok()
                    .or_else(|| {
                        base64::engine::general_purpose::STANDARD
                            .decode(h)
                            .ok()
                            .and_then(|b| serde_json::from_slice(&b).ok())
                    })
            })
            .and_then(|pr| pr.tx_hash)
            .unwrap_or_default();

        debug!(tx_hash = %tx_hash, "x402: payment settled");

        Ok(TransactionReceipt {
            tx_id: if tx_hash.is_empty() {
                uuid::Uuid::new_v4().to_string()
            } else {
                tx_hash
            },
            buyer: ctx.buyer.clone(),
            seller: ctx.seller.clone(),
            dataset_cid: ctx.dataset_cid.clone(),
            price: Price::usdc(amount_raw as f64 / 1_000_000.0),
            protocol: PaymentProtocol::X402,
            timestamp: chrono::Utc::now(),
        })
    }
}

/// Parse CAIP-2 chain ID (e.g. "eip155:8453") or plain number.
fn parse_chain_id(network: &str) -> Result<u64> {
    if let Some(id) = network.strip_prefix("eip155:") {
        return id.parse().context("invalid chain id");
    }
    match network {
        "base" => Ok(8453),
        "base-sepolia" => Ok(84532),
        _ => network.parse().context("unknown network"),
    }
}

fn rand_nonce() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

#[async_trait::async_trait]
impl crate::protocol::PaymentProtocolHandler for X402Client {
    async fn pay(&self, seller_url: &str, ctx: &TransactionContext) -> Result<TransactionReceipt> {
        self.pay(seller_url, ctx).await
    }
}
