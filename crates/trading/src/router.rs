use anyhow::Result;
use data_core::types::*;

use crate::escrow::EscrowClient;
use crate::mpp::MppClient;
use crate::x402::X402Client;

/// Routes payments to the optimal protocol based on transaction context.
pub struct PaymentRouter {
    x402: X402Client,
    mpp: MppClient,
    escrow: EscrowClient,
}

impl PaymentRouter {
    pub fn new(x402: X402Client, mpp: MppClient, escrow: EscrowClient) -> Self {
        Self { x402, mpp, escrow }
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
        PaymentProtocol::X402 // default
    }

    /// Execute a payment using the selected protocol.
    pub async fn pay(
        &self,
        protocol: PaymentProtocol,
        ctx: &TransactionContext,
    ) -> Result<TransactionReceipt> {
        match protocol {
            PaymentProtocol::X402 => self.x402.pay(ctx).await,
            PaymentProtocol::StripeMpp => self.mpp.pay(ctx).await,
            PaymentProtocol::Erc8183 => self.escrow.pay(ctx).await,
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
}
