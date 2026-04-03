// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use data_core::types::*;

use crate::router::TransactionContext;

/// ERC-8183 programmable escrow client — lock → deliver → verify → release.
pub struct EscrowClient;

impl EscrowClient {
    /// Execute an escrowed purchase.
    pub async fn pay(&self, ctx: &TransactionContext) -> Result<TransactionReceipt> {
        // TODO(milestone-4):
        // 1. Call ERC-8183 createJob(buyer, seller, amount, cid, timeout)
        // 2. Funds locked in smart contract
        // 3. Wait for data delivery + Merkle verification
        // 4. Call confirmJob() → release funds
        // 5. On failure → dispute → Evaluator arbitration
        Ok(TransactionReceipt {
            tx_id: uuid::Uuid::new_v4().to_string(),
            buyer: ctx.buyer.clone(),
            seller: ctx.seller.clone(),
            dataset_cid: ctx.dataset_cid.clone(),
            price: Price::usdc(ctx.amount),
            protocol: PaymentProtocol::Erc8183,
            timestamp: chrono::Utc::now(),
            seller_response: None,
        })
    }
}

#[async_trait::async_trait]
impl crate::protocol::PaymentProtocolHandler for EscrowClient {
    async fn pay(&self, _seller_url: &str, ctx: &TransactionContext) -> Result<TransactionReceipt> {
        self.pay(ctx).await
    }
}
