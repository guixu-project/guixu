// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::types::*;

use crate::escrow::EscrowClient;
use crate::mpp::MppClient;
use crate::protocol::PaymentProtocolHandler;
use crate::wallet::AgentWallet;
use crate::x402::X402Client;

/// Routes payments to the optimal protocol based on transaction context.
pub struct PaymentRouter {
    x402: Box<dyn PaymentProtocolHandler>,
    mpp: Box<dyn PaymentProtocolHandler>,
    escrow: Box<dyn PaymentProtocolHandler>,
}

impl PaymentRouter {
    pub fn new(wallet: AgentWallet, testnet: bool) -> Self {
        let escrow = if testnet {
            EscrowClient::for_base_sepolia(wallet.clone())
        } else {
            EscrowClient::for_base_mainnet(wallet.clone())
        };
        Self {
            x402: Box::new(X402Client::new(wallet.clone(), testnet)),
            mpp: Box::new(MppClient::new(wallet)),
            escrow: Box::new(escrow),
        }
    }

    /// Select the best payment protocol for a given transaction.
    pub fn select_protocol(&self, ctx: &TransactionContext) -> PaymentProtocol {
        if ctx.amount < 0.01 && ctx.is_single_request {
            return PaymentProtocol::X402;
        }
        if ctx.is_session_batch || ctx.prefer_fiat {
            return PaymentProtocol::StripeMpp;
        }
        if ctx.amount > 1.0 && ctx.requires_verification {
            return PaymentProtocol::Erc8183;
        }
        PaymentProtocol::X402
    }

    /// Execute a payment using the selected protocol.
    pub async fn pay(
        &self,
        protocol: PaymentProtocol,
        ctx: &TransactionContext,
    ) -> Result<TransactionReceipt> {
        let seller_url = ctx
            .seller_endpoint
            .as_deref()
            .unwrap_or("http://localhost:4242/paid");

        match protocol {
            PaymentProtocol::X402 => self.x402.pay(seller_url, ctx).await,
            PaymentProtocol::StripeMpp => self.mpp.pay(seller_url, ctx).await,
            PaymentProtocol::Erc8183 => self.escrow.pay(seller_url, ctx).await,
        }
    }

    /// Pay with automatic fallback: if x402 fails with 409 (price too high), retry via escrow.
    pub async fn pay_with_fallback(&self, ctx: &TransactionContext) -> Result<TransactionReceipt> {
        let protocol = self.select_protocol(ctx);
        match self.pay(protocol, ctx).await {
            Ok(receipt) => Ok(receipt),
            Err(e) if protocol == PaymentProtocol::X402 && e.to_string().contains("409") => {
                tracing::info!("x402 rejected (price limit), falling back to ERC-8183 escrow");
                self.pay(PaymentProtocol::Erc8183, ctx).await
            }
            Err(e) => Err(e),
        }
    }
}

/// Context for a payment decision.
#[derive(Debug, Clone)]
pub struct TransactionContext {
    pub buyer: Did,
    pub seller: Did,
    pub dataset_cid: DatasetCid,
    pub amount: f64,
    pub is_single_request: bool,
    pub is_session_batch: bool,
    pub prefer_fiat: bool,
    pub requires_verification: bool,
    /// HTTP endpoint of the seller's payment-gated resource.
    pub seller_endpoint: Option<String>,
    /// Additional headers to attach to seller requests (e.g. auth tokens).
    pub seller_headers: Option<Vec<(String, String)>>,
}
