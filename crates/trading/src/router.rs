// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::config::BlockchainConfig;
use data_core::types::*;

use crate::escrow::EscrowClient;
use crate::mpp::MppClient;
use crate::protocol::PaymentProtocolHandler;
use crate::wallet::AgentWallet;
use crate::x402::X402Client;

#[derive(Default)]
pub enum PaymentMode {
    #[default]
    Disabled,
    Enabled {
        wallet: Box<AgentWallet>,
        testnet: bool,
        facilitator_url: String,
    },
}

/// Routes payments to the optimal protocol based on transaction context.
pub struct PaymentRouter {
    mode: PaymentMode,
    x402: Option<Box<dyn PaymentProtocolHandler>>,
    mpp: Option<Box<dyn PaymentProtocolHandler>>,
    escrow: Option<Box<dyn PaymentProtocolHandler>>,
}

impl PaymentRouter {
    pub fn new(mode: PaymentMode) -> Self {
        match mode {
            PaymentMode::Enabled {
                wallet,
                testnet: _,
                facilitator_url: _,
            } => {
                let wallet_clone = (*wallet).clone();
                // We need blockchain config here - this constructor is for legacy use
                // Use from_blockchain_config for proper configuration
                let config = BlockchainConfig::default();
                let escrow = EscrowClient::from_blockchain_config(wallet_clone.clone(), &config);
                let x402 = X402Client::from_blockchain_config(wallet_clone.clone(), &config);
                Self {
                    mode: PaymentMode::Enabled {
                        wallet,
                        testnet: config.is_testnet(),
                        facilitator_url: config.x402_facilitator_url.clone(),
                    },
                    x402: Some(Box::new(x402)),
                    mpp: Some(Box::new(MppClient::new(wallet_clone))),
                    escrow: Some(Box::new(escrow)),
                }
            }
            PaymentMode::Disabled => Self {
                mode: PaymentMode::Disabled,
                x402: None,
                mpp: None,
                escrow: None,
            },
        }
    }

    /// Create PaymentRouter from BlockchainConfig.
    pub fn from_blockchain_config(wallet: AgentWallet, config: &BlockchainConfig) -> Self {
        let wallet_clone = wallet.clone();
        let wallet_box = Box::new(wallet);
        let escrow = EscrowClient::from_blockchain_config(wallet_clone.clone(), config);
        let x402 = X402Client::from_blockchain_config(wallet_clone.clone(), config);
        Self {
            mode: PaymentMode::Enabled {
                wallet: wallet_box,
                testnet: config.is_testnet(),
                facilitator_url: config.x402_facilitator_url.clone(),
            },
            x402: Some(Box::new(x402)),
            mpp: Some(Box::new(MppClient::new(wallet_clone))),
            escrow: Some(Box::new(escrow)),
        }
    }

    pub fn from_wallet(wallet: AgentWallet, testnet: bool) -> Self {
        let mut config = BlockchainConfig::default();
        if testnet {
            config.network = data_core::config::BlockchainNetwork::Sepolia;
        }
        Self::from_blockchain_config(wallet, &config)
    }

    pub fn is_enabled(&self) -> bool {
        matches!(self.mode, PaymentMode::Enabled { .. })
    }

    pub fn mode(&self) -> &PaymentMode {
        &self.mode
    }

    /// Select the best payment protocol for a given transaction.
    pub fn select_protocol(&self, ctx: &TransactionContext) -> PaymentProtocol {
        if !self.is_enabled() {
            return PaymentProtocol::X402;
        }
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
        if !self.is_enabled() {
            anyhow::bail!("payment disabled: cannot pay in disabled mode");
        }

        let seller_url = ctx
            .seller_endpoint
            .as_deref()
            .unwrap_or("http://localhost:4242/paid");

        match protocol {
            PaymentProtocol::X402 => self.x402.as_ref().unwrap().pay(seller_url, ctx).await,
            PaymentProtocol::StripeMpp => self.mpp.as_ref().unwrap().pay(seller_url, ctx).await,
            PaymentProtocol::Erc8183 => self.escrow.as_ref().unwrap().pay(seller_url, ctx).await,
        }
    }

    /// Pay with automatic fallback: if x402 fails with 409 (price too high), retry via escrow.
    pub async fn pay_with_fallback(&self, ctx: &TransactionContext) -> Result<TransactionReceipt> {
        if !self.is_enabled() {
            anyhow::bail!("payment disabled: cannot pay in disabled mode");
        }
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
