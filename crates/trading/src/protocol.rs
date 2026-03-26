use anyhow::Result;
use async_trait::async_trait;
use data_core::types::TransactionReceipt;

use crate::router::TransactionContext;

/// Trait for payment protocol implementations.
/// Allows PaymentRouter to hold `Box<dyn PaymentProtocolHandler>` instead of concrete types.
#[async_trait]
pub trait PaymentProtocolHandler: Send + Sync {
    /// Execute a payment against a seller endpoint.
    async fn pay(
        &self,
        seller_url: &str,
        ctx: &TransactionContext,
    ) -> Result<TransactionReceipt>;
}
