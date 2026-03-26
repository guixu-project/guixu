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
        Self {
            x402: Box::new(X402Client::new(wallet.clone(), testnet)),
            mpp: Box::new(MppClient::new(wallet)),
            escrow: Box::new(EscrowClient),
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
}
