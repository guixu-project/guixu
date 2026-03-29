use anyhow::Result;
use serde_json::json;

use data_core::types::{DatasetCid, PaymentProtocol};
use data_trading::router::TransactionContext;

use crate::server::AppState;

pub async fn handle(args: serde_json::Value, state: &AppState) -> Result<String> {
    let cid_str = args.get("cid").and_then(|v| v.as_str()).unwrap_or("");
    let max_price = args
        .get("max_price")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let cid = DatasetCid(cid_str.to_string());
    let metadata = match state.store.get(&cid)? {
        Some(m) => m,
        None => anyhow::bail!("Dataset {cid_str} not found"),
    };

    if metadata.price.amount > max_price && max_price > 0.0 {
        anyhow::bail!(
            "Price ${:.2} exceeds budget ${:.2}",
            metadata.price.amount,
            max_price
        );
    }

    let tx_ctx = TransactionContext {
        buyer: state.identity.did.clone(),
        seller: metadata.provider.clone(),
        dataset_cid: cid.clone(),
        amount: metadata.price.amount,
        is_single_request: true,
        is_session_batch: false,
        prefer_fiat: false,
        requires_verification: metadata.price.amount > 1.0,
        seller_endpoint: args
            .get("seller_endpoint")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                // Auto-resolve x402 endpoint for Guixu Hub datasets
                if let Some(listing_id) = cid_str.strip_prefix("guixu-hub:") {
                    let base = std::env::var("GUIXU_HUB_BASE_URL")
                        .unwrap_or_else(|_| "https://www.guixu.org".into());
                    Some(format!("{base}/api/x402/{listing_id}"))
                } else {
                    None
                }
            }),
    };

    let (receipt, protocol_desc, selection_reason) = if metadata.price.is_free() {
        (
            None,
            "Free dataset — no payment required",
            "price=0, no payment needed",
        )
    } else {
        let protocol = state.payment_router.select_protocol(&tx_ctx);
        let r = state.payment_router.pay(protocol, &tx_ctx).await?;
        let (desc, reason) = match protocol {
            PaymentProtocol::X402 => (
                "Micropayment via x402 (USDC on Base L2)",
                "amount<$0.01 single request → x402 micropayment",
            ),
            PaymentProtocol::StripeMpp => (
                "Session payment via Stripe MPP",
                "session batch or fiat preferred → Stripe MPP",
            ),
            PaymentProtocol::Erc8183 => (
                "Escrowed payment via ERC-8183 (verify then release)",
                "amount>$1.00 with verification → ERC-8183 escrow",
            ),
        };
        (Some(r), desc, reason)
    };

    let tx_id = receipt
        .as_ref()
        .map(|r| r.tx_id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let protocol_name = receipt
        .as_ref()
        .map(|r| format!("{:?}", r.protocol))
        .unwrap_or_else(|| "none".into());

    let delivery = if let Some(download_url) = receipt
        .as_ref()
        .and_then(|r| r.seller_response.as_ref())
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok())
        .and_then(|v| v.get("downloadUrl")?.as_str().map(String::from))
    {
        json!({
            "method": "url",
            "download_url": download_url,
        })
    } else {
        match state.store.get_file_path(&cid)? {
            Some(path) if path.exists() => {
                json!({
                    "method": "local",
                    "file_path": path.to_string_lossy(),
                    "size_bytes": std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                })
            }
            _ => {
                let download_dir = state
                    .store
                    .get_file_path(&DatasetCid("__config_data_dir__".into()))?
                    .unwrap_or_else(|| std::path::PathBuf::from("/tmp/guixu-downloads"));
                let dest =
                    download_dir.join(format!("{}.dat", &cid_str[..16.min(cid_str.len())]));
                json!({
                    "method": "torrent_pending",
                    "info_hash": metadata.info_hash,
                    "download_path": dest.to_string_lossy(),
                })
            }
        }
    };

    Ok(json!({
        "status": "purchased",
        "cid": cid_str,
        "price_paid": metadata.price.amount,
        "payment_protocol": protocol_name,
        "protocol_description": protocol_desc,
        "protocol_selection_reason": selection_reason,
        "budget_check": if max_price > 0.0 {
            format!("${:.2} <= budget ${:.2}", metadata.price.amount, max_price)
        } else {
            "no budget limit set".into()
        },
        "tx_id": tx_id,
        "on_chain_receipt": "EAS attestation simulated (Base L2)",
        "delivery": delivery,
    })
    .to_string())
}
