// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Context, Result};
use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use data_core::types::*;

use crate::router::TransactionContext;
use crate::wallet::AgentWallet;

/// Stripe Machine Payments Protocol client — session-based streaming payments.
///
/// MPP flow (crypto mode on Tempo):
/// 1. GET seller endpoint → receive 402 + payment challenge
/// 2. Parse challenge (amount, recipient, challengeId, method)
/// 3. Sign the challenge with agent wallet to create a credential
/// 4. Retry with `Authorization: MPP <credential>` header
/// 5. Receive 200 + receipt header with settlement confirmation
pub struct MppClient {
    wallet: AgentWallet,
    http: Client,
}

/// An active MPP session for batch transactions.
pub struct MppSession {
    pub session_id: String,
    pub budget_remaining: f64,
    challenges_completed: u64,
}

/// 402 challenge returned by an MPP-enabled endpoint.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MppChallenge {
    #[serde(alias = "challengeId")]
    challenge_id: String,
    /// Required payment amount (e.g. "0.01").
    amount: String,
    /// Recipient deposit address.
    #[serde(alias = "request")]
    request: Option<MppRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MppRequest {
    #[allow(dead_code)]
    recipient: Option<String>,
    amount: Option<String>,
    #[allow(dead_code)]
    currency: Option<String>,
}

/// Credential sent in the Authorization header.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MppCredential {
    challenge_id: String,
    from: String,
    signature: String,
    amount: String,
}

/// Receipt from the response header.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MppReceipt {
    #[serde(alias = "paymentId")]
    payment_id: Option<String>,
    #[serde(alias = "txHash")]
    tx_hash: Option<String>,
    #[allow(dead_code)]
    status: Option<String>,
}

impl MppClient {
    pub fn new(wallet: AgentWallet) -> Self {
        Self {
            wallet,
            http: Client::new(),
        }
    }

    /// Create a new MPP session with a budget cap.
    pub fn create_session(&self, budget: f64) -> MppSession {
        MppSession {
            session_id: uuid::Uuid::new_v4().to_string(),
            budget_remaining: budget,
            challenges_completed: 0,
        }
    }

    /// Execute a single MPP payment against a seller endpoint.
    pub async fn pay(
        &self,
        seller_url: &str,
        ctx: &TransactionContext,
    ) -> Result<TransactionReceipt> {
        // Step 1: Initial request → expect 402
        let resp = self.http.get(seller_url).send().await?;

        if resp.status().as_u16() != 402 {
            bail!(
                "mpp: expected 402 from seller, got {}",
                resp.status().as_u16()
            );
        }

        // Step 2: Parse the 402 challenge body
        let body = resp.text().await?;
        let challenge: MppChallenge =
            serde_json::from_str(&body).context("mpp: failed to parse 402 challenge")?;

        let amount_str = challenge
            .request
            .as_ref()
            .and_then(|r| r.amount.as_deref())
            .unwrap_or(&challenge.amount);

        let amount: f64 = amount_str
            .trim_start_matches('$')
            .parse()
            .context("mpp: invalid amount")?;

        info!(
            challenge_id = %challenge.challenge_id,
            amount,
            "mpp: signing payment credential"
        );

        // Step 3: Sign the challenge to create a credential
        let sign_payload = format!(
            "{}:{}:{}",
            challenge.challenge_id,
            self.wallet.address(),
            amount_str
        );
        let signature = self.wallet.sign_message(sign_payload.as_bytes()).await?;
        let sig_hex = format!("0x{}", hex::encode(&signature));

        let credential = MppCredential {
            challenge_id: challenge.challenge_id.clone(),
            from: format!("{}", self.wallet.address()),
            signature: sig_hex,
            amount: amount_str.to_string(),
        };

        let encoded =
            base64::engine::general_purpose::STANDARD.encode(serde_json::to_string(&credential)?);

        // Step 4: Retry with Authorization header
        let resp = self
            .http
            .get(seller_url)
            .header("authorization", format!("MPP {encoded}"))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("mpp: payment rejected with {status}: {body}");
        }

        // Step 5: Parse receipt
        let receipt = resp
            .headers()
            .get("x-payment-receipt")
            .or_else(|| resp.headers().get("payment-response"))
            .and_then(|h| h.to_str().ok())
            .and_then(|h| serde_json::from_str::<MppReceipt>(h).ok());

        let tx_id = receipt
            .as_ref()
            .and_then(|r| r.tx_hash.as_deref().or(r.payment_id.as_deref()))
            .unwrap_or("")
            .to_string();

        debug!(tx_id = %tx_id, "mpp: payment settled");

        Ok(TransactionReceipt {
            tx_id: if tx_id.is_empty() {
                uuid::Uuid::new_v4().to_string()
            } else {
                tx_id
            },
            buyer: ctx.buyer.clone(),
            seller: ctx.seller.clone(),
            dataset_cid: ctx.dataset_cid.clone(),
            price: Price::usdc(amount),
            protocol: PaymentProtocol::StripeMpp,
            timestamp: chrono::Utc::now(),
            seller_response: None,
        })
    }

    /// Pay within an existing session, decrementing the budget.
    pub async fn pay_session(
        &self,
        session: &mut MppSession,
        seller_url: &str,
        ctx: &TransactionContext,
    ) -> Result<TransactionReceipt> {
        if session.budget_remaining < ctx.amount {
            bail!(
                "mpp: session budget exhausted ({:.4} remaining, need {:.4})",
                session.budget_remaining,
                ctx.amount
            );
        }

        let receipt = self.pay(seller_url, ctx).await?;
        session.budget_remaining -= receipt.price.amount;
        session.challenges_completed += 1;
        Ok(receipt)
    }
}

#[async_trait::async_trait]
impl crate::protocol::PaymentProtocolHandler for MppClient {
    async fn pay(&self, seller_url: &str, ctx: &TransactionContext) -> Result<TransactionReceipt> {
        self.pay(seller_url, ctx).await
    }
}
